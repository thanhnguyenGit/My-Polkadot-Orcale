// std
use std::{sync::Arc, time::Duration};
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::thread::sleep;
use codec::{Decode, Encode, Error, MaxEncodedLen};
use frame_benchmarking::__private::storage::bounded_vec::BoundedVec;
use frame_benchmarking::__private::traits::tasks::__private::TypeInfo;
use futures::FutureExt;
use jsonrpsee::tokio::sync::mpsc;
use jsonrpsee::tokio::time::{sleep_until, Instant};
// Substrate Imports
use sc_client_api::Backend;
use sc_service::{Configuration, PartialComponents, SpawnTaskHandle, TFullBackend, TFullClient, TaskManager};
use sp_api::__private::scale_info;
use sp_core::offchain::{OffchainStorage, StorageKind};
use sp_core::RuntimeDebug;
use sp_io::offchain::local_storage_set;
use parachain_template_runtime::{
    apis::RuntimeApi,
    opaque::{Block, Hash},
};
use substrate_api_client::api::api_client::Api;
use sc_client_db::offchain;
use sp_runtime::print;
// local import
use model::wasm_compatiable::Payload;

const OCW_STORAGE_PREFIX: &[u8] = b"storage";
const WASMSTORE_KEY_LIST: &[u8] = b"wasmstore_jobs_executor";
#[derive(Debug)]
enum StorageError {
    FailToWrite,
    ParsingError,
    NoPendingPayloadFound,
}

struct JobManager {
    job_pool: BTreeMap<Vec<u8>, Payload>
}

pub fn run_executor(task_manager: &TaskManager,backend : Arc<TFullBackend<Block>>) {
    let group = "OffChainService";
    let executor = task_manager.spawn_handle();
    executor.spawn("OffChainMonitor", group, offchain_monitor(backend.clone()).boxed());

}

async fn offchain_monitor(backend : Arc<TFullBackend<Block>>) {
    let mut offchain_db = backend.offchain_storage().expect("No storage found");
    offchain_db.set(OCW_STORAGE_PREFIX, WASMSTORE_KEY_LIST, b"");
    loop {
        if let Some(meta_val) = read_from_offchain_db(backend.clone(),WASMSTORE_KEY_LIST).await {
            println!("Receive key_list {:?}", meta_val);
            match key_list_parser(&meta_val) {
                Ok(val) => {
                    println!("Payload: {:?}", val);
                }
                Err(_) => {
                    println!("{:?} - msg: No pending payload in storage", StorageError::NoPendingPayloadFound);
                }
            }
        }

        sleep_until(Instant::now() + Duration::from_millis(6000)).await;
    }
}

async fn do_task(name: &'static str) {
    println!("Hello world from {}",name);
    sleep_until(Instant::now() + Duration::from_millis(200)).await;
}


async fn process_script(backend: Arc<TFullBackend<Block>>) {
    let mut offchain_db = backend.offchain_storage().expect("No storage found");

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

fn key_list_parser(key_list_raw : &[u8]) -> Result<Payload, StorageError>
{
    match Payload::decode(&mut &key_list_raw[..]) {
        Ok(val) => {
            Ok(val)
        }
        Err(e) => {
            println!("Error decoding the payload from service {:?}", e);
            Err(StorageError::ParsingError)
        }
    }
}
