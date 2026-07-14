use std::collections::HashMap;
use std::hash::Hash;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FirstSeenResult {
    First,
    Duplicate,
    Conflicting,
}

pub(crate) struct FirstSeen<K> {
    indexes: HashMap<K, usize>,
}

impl<K> FirstSeen<K>
where
    K: Eq + Hash,
{
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            indexes: HashMap::with_capacity(capacity),
        }
    }

    pub(crate) fn push<T>(&mut self, values: &mut Vec<T>, key: K, value: T) -> FirstSeenResult
    where
        T: PartialEq,
    {
        if let Some(index) = self.indexes.get(&key) {
            return if values.get(*index) == Some(&value) {
                FirstSeenResult::Duplicate
            } else {
                FirstSeenResult::Conflicting
            };
        }

        self.indexes.insert(key, values.len());
        values.push(value);
        FirstSeenResult::First
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_first_position_without_reordering_or_cloning_values() {
        let mut seen = FirstSeen::with_capacity(3);
        let mut values = Vec::new();

        assert_eq!(seen.push(&mut values, 1, "first"), FirstSeenResult::First);
        assert_eq!(
            seen.push(&mut values, 1, "first"),
            FirstSeenResult::Duplicate
        );
        assert_eq!(
            seen.push(&mut values, 1, "changed"),
            FirstSeenResult::Conflicting
        );
        assert_eq!(seen.push(&mut values, 2, "second"), FirstSeenResult::First);
        assert_eq!(values, ["first", "second"]);
    }
}
