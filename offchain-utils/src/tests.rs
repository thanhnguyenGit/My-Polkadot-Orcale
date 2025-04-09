#[cfg(test)]
mod tests {
    use super::*;
    use sp_io::TestExternalities;

    #[test]
    fn test_fetch_api_key() {
        // Set up a mock environment
        let mut t = TestExternalities::default();
        t.execute_with(|| {
            // Store a fake API key in the offchain storage
            sp_io::offchain::local_storage_set(
                sp_io::offchain::StorageKind::PERSISTENT,
                b"test_api_key",
                b"mock_key_value",
            );

            // Fetch the key using the trait
            let result = DefaultOffchainApiKey::fetch_api_key_for_request("test_api_key");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "mock_key_value");
        });
    }

    #[test]
    fn test_missing_api_key() {
        let mut t = TestExternalities::default();
        t.execute_with(|| {
            // Attempt to fetch a non-existent key
            let result = DefaultOffchainApiKey::fetch_api_key_for_request("non_existent_key");
            assert!(result.is_err());
        });
    }
}
