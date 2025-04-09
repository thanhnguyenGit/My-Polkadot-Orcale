use sp_io::offchain::{local_storage_get, local_storage_set};

use sp_core::offchain::StorageKind::PERSISTENT;

use scale_info::prelude::string::String;

/// A trait representing a way to fetch an API key for offchain calls.
pub trait OffchainApiKey {
	/// Fetch the API key for a given `key`.
	fn fetch_api_key_for_request(key: &str) -> Result<String, &'static str> {
		Self::do_fetch_local(key).ok_or("Missing or invalid API key")
	}

	/// Fetch the raw key string from local offchain storage.
	fn do_fetch_local(key: &str) -> Option<String> {
		if let Some(api_key_bytes) = local_storage_get(PERSISTENT, key.as_bytes()) {
			let api_key = String::from_utf8(api_key_bytes).expect("Invalid UTF-8");
			Some(api_key)
		} else {
			log::error!("API key not found in offchain storage");
			None
		}
	}

	/// Store an API key in local offchain storage for a given `key`.
	fn store_api_key(key: &str, value: &str) {
		local_storage_set(PERSISTENT, key.as_bytes(), value.as_bytes());
	}
}

/// A default implementation of `OffchainApiKey` using local offchain storage.
pub struct DefaultOffchainApiKey;

impl OffchainApiKey for DefaultOffchainApiKey {}
