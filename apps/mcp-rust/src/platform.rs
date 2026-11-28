//! Unified AI contract: static capabilities, not evidence of provider availability.
use serde_json::Value;
pub fn manifest()->Value{
    serde_json::from_str(include_str!("../contracts/ai-platform-v1.json")).expect("checked-in platform contract")
}
#[cfg(test)]mod tests{
    #[test]fn contract_matches_runtime(){
        let c=super::manifest();
        assert_eq!(c["version"],1);
        assert_eq!(c["limits"]["model_steps"],crate::agent::MAX_STEPS);
        assert_eq!(c["limits"]["tool_calls"],crate::agent::MAX_CALLS);
        assert_eq!(c["limits"]["children"],crate::team::MAX_CHILDREN);
        assert_eq!(c["limits"]["model_attempts"],crate::team::MAX_ATTEMPTS);
        assert_eq!(c["capabilities"]["native_authenticated"],false);
    }
}
