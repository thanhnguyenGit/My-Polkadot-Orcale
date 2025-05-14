// std
use std::{sync::Arc, time::Duration};
use std::collections::{BTreeMap, VecDeque};
use std::process::{Command, Stdio};
use std::thread::sleep;
use codec::{decode_vec_with_len, Decode, Encode, Error, MaxEncodedLen};
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
use sp_io::offchain::{local_storage_set, timestamp};
use parachain_template_runtime::{
    apis::RuntimeApi,
    opaque::{Block, Hash},
};
use substrate_api_client::api::api_client::Api;
use sc_client_db::offchain;
use sp_io::misc::print_num;
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

#[derive(Default)]
struct JobPool {

    job_pool: Vec<Vec<u8>>
}

pub fn run_executor(task_manager: &mut TaskManager, backend : Arc<TFullBackend<Block>>) {
    let group = "OffChainService";
    let (tx,rx) = mpsc::channel::<Payload>(20);
    let mut offchain_db = backend.offchain_storage().expect("No storage found");
    offchain_db.set(OCW_STORAGE_PREFIX, WASMSTORE_KEY_LIST, b"init");
    let executor = Arc::new(task_manager.spawn_handle());

    let job_pool = Arc::new(JobPool::default());

    executor.clone().spawn("OffChainMonitor", group, async move {
        loop {
            if let Some(key_lists) = read_from_offchain_db(backend.clone(),WASMSTORE_KEY_LIST).await {
                println!("Receive key_list {:?}", key_lists);
                match key_list_parser(&key_lists) {
                    Ok(key_list) => {
                        for i in key_list.iter() {
                            let taks_backend = backend.clone();
                            let value = i.clone();
                            executor.spawn("",group,async move {
                                let val = value.as_slice();
                                if let Some(val) = read_from_offchain_db(taks_backend.clone(), val).await {
                                    println!("Value: {:?}", val);
                                }

                                sleep_until(Instant::now() + Duration::from_millis(600)).await;
                            })
                        }
                    }
                    Err(_) => {
                        println!("{:?} - msg: No pending payload in storage", StorageError::NoPendingPayloadFound);
                    }
                }
            }

            sleep_until(Instant::now() + Duration::from_millis(6000)).await;
        }
    });
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

fn key_list_parser(key_list_raw : &[u8]) -> Result<Vec<Vec<u8>>, StorageError>
{
    let key_list = &key_list_raw[4..];
    let chunks = key_list.chunks(32).map(|chunk| {
        chunk.to_vec()
    }).collect::<Vec<Vec<u8>>>();

    Ok(chunks)
}
