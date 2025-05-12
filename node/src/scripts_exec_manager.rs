// std
use std::{sync::Arc, time::Duration};
use std::process::{Command, Stdio};
use std::thread::sleep;
use codec::{Decode, Encode, Error, MaxEncodedLen};
use frame_benchmarking::__private::storage::bounded_vec::BoundedVec;
use frame_benchmarking::__private::traits::tasks::__private::TypeInfo;
use jsonrpsee::tokio::time::{sleep_until, Instant};
// Substrate Imports
use sc_client_api::Backend;
use sc_service::{Configuration, PartialComponents, TFullBackend, TFullClient, TaskManager};
use sp_api::__private::scale_info;
use sp_core::offchain::{OffchainStorage, StorageKind};
use sp_core::RuntimeDebug;
use sp_io::offchain::local_storage_set;
use parachain_template_runtime::{
    apis::RuntimeApi,
    opaque::{Block, Hash},
};
use substrate_api_client::api::api_client::Api;
// local import
use model::wasm_compatiable::Payload;

const OCW_STORAGE_PREFIX: &[u8] = b"storage";
const WASMSTORE_KEY_LIST: &[u8] = b"wasmstore_jobs_executor";
enum StorageError {
    FailToWrite,
    ParsingError,
}

pub async fn offchain_storage_monitoring(backend: Arc<TFullBackend<Block>>) {
    let mut offchain_db = backend.offchain_storage().expect("No storage found");
    offchain_db.set(OCW_STORAGE_PREFIX, WASMSTORE_KEY_LIST, b"init");
    loop {
        if let Some(meta_val) = offchain_db.get(OCW_STORAGE_PREFIX,WASMSTORE_KEY_LIST) {
            println!("Receive key_list {:?}", meta_val);
            let x = key_list_parser(&meta_val);

        }
        sleep_until(Instant::now() + Duration::from_millis(6000)).await;
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

fn key_list_parser<T,State>(key_list_raw : &T) -> Result<Payload<State>, StorageError>
where
    T : Encode + Decode + AsRef<[u8]>,
    State : TypeInfo + Encode + Decode + MaxEncodedLen
{
    match Payload::<State>::decode(&mut &key_list_raw[..]) {
        Ok(val) => {
            Ok(val)
        }
        Err(e) => {
            println!("Error decoding the payload from service {:?}", e);
            Err(StorageError::ParsingError)
        }
    }
}
