use std::collections::HashMap;

#[derive(Debug, Default)]
pub(super) struct RevisionRegistry {
    revisions: HashMap<String, u64>,
}

impl RevisionRegistry {
    pub(super) fn current(&self, target: &str) -> String {
        revision_token(self.revisions.get(target).copied().unwrap_or(1))
    }

    pub(super) fn bump(&mut self, target: &str) -> String {
        let revision = self.revisions.entry(target.to_string()).or_insert(1);
        *revision += 1;
        revision_token(*revision)
    }
}

fn revision_token(revision: u64) -> String {
    format!("r_{revision:x}")
}

#[cfg(test)]
mod tests {
    use super::RevisionRegistry;

    #[test]
    fn revisions_are_stable_per_target_until_bumped() {
        let mut revisions = RevisionRegistry::default();

        assert_eq!(revisions.current("page-a"), "r_1");
        assert_eq!(revisions.current("page-a"), "r_1");
        assert_eq!(revisions.bump("page-a"), "r_2");
        assert_eq!(revisions.current("page-a"), "r_2");
        assert_eq!(revisions.current("page-b"), "r_1");
    }
}
