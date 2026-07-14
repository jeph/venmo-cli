#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Observed<Outcome, State> {
    outcome: Outcome,
    state: State,
}

impl<Outcome, State> Observed<Outcome, State> {
    pub(crate) const fn new(outcome: Outcome, state: State) -> Self {
        Self { outcome, state }
    }
}
