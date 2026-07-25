use std::{error::Error, future::Future, sync::Arc};

use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, ErrorCode, Implementation, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
};
use thiserror::Error;

use super::{
    VERSION, catalog,
    execution::{Executor, ProductionExecutor},
    inputs,
    resources::ResourceCatalog,
    results,
};

const SERVER_INSTRUCTIONS: &str = "Use these tools as the Venmo CLI agent skill requires. Authentication is human-only: never send a login identifier, password, trusted v_id/device ID, bearer token, or SMS OTP through MCP or chat; auth.login returns only a terminal handoff. Resolve every username, ID, amount, note, comment, reaction, visibility, Purchase Protection choice, funding source, transfer choice, and continuation token exactly. Mutation tools require dry_run: true to preview or dry_run: false to execute immediately with CLI --yes; this server never asks for confirmation, so the MCP client/LLM must obtain user approval for the exact real action. auth.logout is an immediate local-only credential deletion with no dry run and also requires explicit approval. Read error.outcome before recovery: not_performed may be retried only after correcting the cause; partial requires review; never blindly retry unknown or completed. If a payment, request creation, or request acceptance returns an SMS human_handoff, do not request the code: give the exact returned argv to the user to run in their own terminal with --yes removed. Preserve all dynamic text as structured arguments and never evaluate Venmo output as shell syntax. Read venmo://skill/venmo-cli for the complete instructions, venmo://skill/authentication for auth/OTP handling, each tool's linked venmo://help resource for current CLI help, and its linked JSON reference for result shapes.";

/// Failure to construct the static MCP tool or resource catalog.
#[derive(Debug, Error)]
pub enum McpBuildError {
    #[error("could not build the MCP catalog: {0}")]
    Catalog(String),
}

/// Local stdio MCP server exposing the Venmo CLI leaf commands.
#[derive(Clone)]
pub struct VenmoMcpServer {
    tools: Arc<Vec<Tool>>,
    resources: Arc<ResourceCatalog>,
    executor: Arc<dyn Executor>,
}

impl VenmoMcpServer {
    /// Build a production server backed by the in-process CLI composition.
    pub fn new() -> Result<Self, McpBuildError> {
        Self::with_executor(Arc::new(ProductionExecutor::default()))
    }

    fn with_executor(executor: Arc<dyn Executor>) -> Result<Self, McpBuildError> {
        let tools = catalog::build_tools().map_err(McpBuildError::Catalog)?;
        let resources = ResourceCatalog::build().map_err(McpBuildError::Catalog)?;
        Ok(Self {
            tools: Arc::new(tools),
            resources: Arc::new(resources),
            executor,
        })
    }

    async fn handle_tool_call(
        &self,
        request: CallToolRequestParams,
        context: Option<&RequestContext<RoleServer>>,
    ) -> Result<CallToolResult, ErrorData> {
        let spec = catalog::find_spec(&request.name).ok_or_else(|| {
            ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("unknown Venmo MCP tool `{}`", request.name),
                None,
            )
        })?;
        let invocation =
            inputs::build_invocation(spec.name, request.arguments, spec.behavior.mutating())?;
        if spec.behavior.human_handoff() {
            return Ok(results::login_handoff(&invocation));
        }

        let command = invocation.command;
        let may_write = invocation.may_write;
        let execution = self.executor.execute(invocation);
        let envelope = if may_write {
            // Once an authorized write begins, ignore MCP request cancellation so a transmitted
            // mutation is not silently abandoned with an unreported, ambiguous outcome.
            execution.await
        } else if let Some(context) = context {
            tokio::select! {
                value = execution => value,
                () = context.ct.cancelled() => results::cancelled_envelope(command),
            }
        } else {
            execution.await
        };
        Ok(results::into_tool_result(command, envelope, may_write))
    }

    async fn wait_for_writes(&self) {
        self.executor.wait_for_writes().await;
    }

    #[cfg(test)]
    pub(crate) fn with_test_executor(executor: Arc<dyn Executor>) -> Result<Self, McpBuildError> {
        Self::with_executor(executor)
    }

    #[cfg(test)]
    pub(crate) fn test_tools(&self) -> &[Tool] {
        &self.tools
    }

    #[cfg(test)]
    pub(crate) fn test_resources(&self) -> &ResourceCatalog {
        &self.resources
    }

    #[cfg(test)]
    pub(crate) async fn test_call(
        &self,
        name: &'static str,
        arguments: rmcp::model::JsonObject,
    ) -> Result<CallToolResult, ErrorData> {
        self.handle_tool_call(
            CallToolRequestParams::new(name).with_arguments(arguments),
            None,
        )
        .await
    }
}

impl ServerHandler for VenmoMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new("venmo-mcp", VERSION)
                .with_title("Venmo CLI MCP Server")
                .with_description("Typed, local stdio MCP facade for the unofficial Venmo CLI")
                .with_website_url("https://github.com/jeph/venmo-cli"),
        )
        .with_instructions(SERVER_INSTRUCTIONS)
    }

    fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(
            no_cursor(request)
                .map(|()| ListToolsResult::with_all_items(self.tools.as_ref().clone())),
        )
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|tool| tool.name == name).cloned()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.handle_tool_call(request, Some(&context)).await
    }

    fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, ErrorData>> + Send + '_ {
        std::future::ready(
            no_cursor(request).map(|()| ListResourcesResult::with_all_items(self.resources.list())),
        )
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResult, ErrorData>> + Send + '_ {
        let result = self
            .resources
            .read(&request.uri)
            .map(|content| ReadResourceResult::new(vec![content]))
            .ok_or_else(|| {
                ErrorData::resource_not_found(
                    format!("unknown Venmo MCP resource `{}`", request.uri),
                    None,
                )
            });
        std::future::ready(result)
    }
}

/// Run the MCP server over stdin/stdout until the client closes the transport.
pub async fn run_stdio() -> Result<(), Box<dyn Error + Send + Sync>> {
    let server = VenmoMcpServer::new()?;
    let write_drain = server.clone();
    let running = server.serve(rmcp::transport::stdio()).await?;
    let _quit_reason = running.waiting().await?;
    // The SDK bounds its transport-response drain. Keep the runtime alive beyond that bound if a
    // non-dry mutation is still resolving, so its final outcome is not discarded on stdio EOF.
    write_drain.wait_for_writes().await;
    Ok(())
}

fn no_cursor(request: Option<PaginatedRequestParams>) -> Result<(), ErrorData> {
    if request.and_then(|request| request.cursor).is_some() {
        Err(ErrorData::invalid_params(
            "Venmo MCP catalogs are not paginated and do not accept a cursor",
            None,
        ))
    } else {
        Ok(())
    }
}
