// std
use std::{sync::Arc, time::Duration};
use std::process::{Command, Stdio};
use std::thread::sleep;
use codec::{Decode, Encode};
use jsonrpsee::tokio::time::{sleep_until, Instant};
// Substrate Imports
use sc_client_api::Backend;
use sc_service::{Configuration, PartialComponents, TFullBackend, TFullClient, TaskManager};
use sp_core::offchain::OffchainStorage;
use parachain_template_runtime::{
    apis::RuntimeApi,
    opaque::{Block, Hash},
};



const OCW_STORAGE_PREFIX: &[u8] = b"storage";
const WASMSTORE_META_KEY: &[u8] = b"wasmstore::meta";
enum StorageError {
    FailToWrite,
}

#[derive(Encode,Decode)]
struct Meta {

}

pub async fn offchain_storage_monitoring(backend: Arc<TFullBackend<Block>>) {
    let offchain_db = backend.offchain_storage().expect("No storage found");
    loop {
        if let Some(meta_val) = offchain_db.get(OCW_STORAGE_PREFIX,WASMSTORE_META_KEY) {
            println!("Receive key {:?}", meta_val);
        }
        sleep_until(Instant::now() + Duration::from_millis(5000)).await;
    }
}

async fn read_from_offchain_db(backend: Arc<TFullBackend<Block>>, key: &[u8]) -> Option<Vec<u8>>{
    let offchain_db = backend.offchain_storage().expect("No LocalStorage found");

    if let Some(val) = offchain_db.get(OCW_STORAGE_PREFIX, key) {
        Some(val)
    } else {
        None
    }
}

async fn write_to_offchain_db(backend: Arc<TFullBackend<Block>>, key: &[u8], new_value: &[u8]) -> Result<(),StorageError>{
    let mut offchain_db = backend.offchain_storage().expect("Offchain storage exist");

    if let Some(val) = offchain_db.get(OCW_STORAGE_PREFIX,key) {
        match offchain_db.compare_and_set(OCW_STORAGE_PREFIX, key, Some(&val), new_value) {
            true => {
                log::info!("Successfully write to storage: old value - {:?}, new value - {:?}",val,new_value);
                Ok(())
            }
            false => {
                log::info!("Fail to write to storage: old value - {:?}",val);
                Err(StorageError::FailToWrite)
            }
        }
    } else {
        offchain_db.set(OCW_STORAGE_PREFIX, key,new_value);
        log::info!("Successfully write to storage new value - {:?}",new_value);
        Ok(())
    }
}

