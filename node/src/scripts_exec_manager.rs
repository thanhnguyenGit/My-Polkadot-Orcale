// std
use std::{sync::Arc, time::Duration};
use std::borrow::Cow;
use std::collections::{BTreeMap, VecDeque};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::thread::sleep;
use codec::{decode_vec_with_len, Decode, Encode, Error, MaxEncodedLen};
use frame_benchmarking::__private::storage::bounded_vec::BoundedVec;
use frame_benchmarking::__private::traits::tasks::__private::TypeInfo;
use frame_benchmarking::benchmarking::current_time;
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
use sp_runtime::{print, KeyTypeId};
use sc_keystore::LocalKeystore;
use sp_core::crypto::key_types;
use sp_core::ecdsa::Public;
use sp_keystore::{Keystore, KeystorePtr};

// local import
use model::wasm_compatiable::{JobState, RequestPayload, ResponePayload};
use script_executor::runtime::abi_reader;
const OCW_STORAGE_PREFIX: &[u8] = b"storage";
const WASMSTORE_KEY_LIST: &[u8] = b"wasmstore_jobs_executor";
const WASMSTORE_RESULT_LIST: &[u8] = b"wasmstore_jobs_result";

const CHARLIES_KEYSTORE_PATH : &str = "/tmp/zombie-3d89086b232a9b7a9448c265be86af6d_-27920-gm7XFZMCV1bS/charlie/data/chains/custom/keystore";
#[derive(Debug)]
enum StorageError {
    FailToWrite,
    ParsingError,
    NoPendingPayloadFound,
    WasmProcessingError,
    JobExist,
}

struct JobPool {
    result_rx : mpsc::Receiver<JobResult>,
}
#[derive(Decode,Encode,Debug)]
struct JobResult {
    job_id : Vec<u8>,
    result : Vec<u8>
}
impl JobPool {
    fn new(receiver: mpsc::Receiver<JobResult>) -> JobPool {
        JobPool {
            result_rx: receiver
        }
    }
    async fn result_listener(&mut self,backend : Arc<TFullBackend<Block>>) {
        loop {
            if let Some(job_result) = self.result_rx.recv().await {
                let job_id = job_result.job_id;
                let result = job_result.result;
                let respone = ResponePayload {
                    job_id: job_id.clone(),
                    job_result: result,
                    job_state: JobState::Finish,
                };
                println!("Receive Job result: {:?}", job_id);
                match write_to_offchain_db(backend.clone(), &job_id, &respone.encode()).await {
                    Ok(_) => {
                        println!("Write result to local storage: {:?}", respone);
                    }
                    Err(e) => {
                        println!("{:?} - msg: Failed to write job: {:?} to local storage", e, job_id)
                    }
                }
                if let Some(value) = read_from_offchain_db(backend.clone(), WASMSTORE_RESULT_LIST).await {
                    let mut result = value;
                    result.extend_from_slice(&job_id);
                    println!("Result_key_list Value {:?}",result);
                    let _ = write_to_offchain_db(backend.clone(), WASMSTORE_RESULT_LIST, &result).await;
                }
            }
        }
    }
}

fn check_keystore(keystore: Arc<dyn Keystore>) {
    const KEY_TYPE_BABE: KeyTypeId = KeyTypeId(*b"aura");

    // Retrieve BABE session keys for underlying collator
    let public_keys = keystore
        .sr25519_public_keys(KEY_TYPE_BABE);
    let signed_payload = keystore.sr25519_sign(KEY_TYPE_BABE, &public_keys[0], "BRIv".as_ref());
    let x = signed_payload.unwrap().unwrap().0;
}

pub fn run_executor(task_manager: &mut TaskManager, backend : Arc<TFullBackend<Block>>,keystore: Arc<dyn Keystore>) {
    let group = "OffChainService";
    let mut offchain_db = backend.offchain_storage().expect("No storage found");
    offchain_db.set(OCW_STORAGE_PREFIX, WASMSTORE_KEY_LIST, b"init");
    offchain_db.set(OCW_STORAGE_PREFIX, WASMSTORE_RESULT_LIST, b"init");
    let executor = Arc::new(task_manager.spawn_handle());

    check_keystore(keystore.clone());

    let (job_tx,job_rx) = mpsc::channel::<JobResult>(20);
    let mut job_pool = JobPool::new(job_rx);

    let backend_for_job_pool = backend.clone();
    executor.clone().spawn("JobPoolMonitor", group, async move {
        job_pool.result_listener(backend_for_job_pool).await
    });

    executor.clone().spawn("OffChainMonitor", group, async move {
        loop {
            if let Some(key_lists) = read_from_offchain_db(backend.clone(),WASMSTORE_KEY_LIST).await {
                println!("Receive key_list {:?}", key_lists);
                // Parse key-list, because this is K-V store of bytes data.
                // Parse the value to get list of jobs.
                match key_list_parser(&key_lists) {
                    Ok(key_list) => {
                        // clear the key_list in K-V
                        let _ = write_to_offchain_db(backend.clone(), WASMSTORE_KEY_LIST, b"init").await;
                        // Iter over key_list to assigned job to spawned task.
                        for (_,i) in key_list.iter().enumerate() {
                            let task_backend = backend.clone();
                            let value = i.clone();
                            let tx = job_tx.clone();
                            let val = value.as_slice();
                            let encoded_payload = read_from_offchain_db(task_backend.clone(), val).await;
                            executor.clone().spawn_blocking("","",async move {
                                if let Some(res) = encoded_payload {
                                    match RequestPayload::decode(&mut &res[..]) {
                                        Ok(payload) => {
                                            println!("Payload Job id: {:?}", payload.job_id);
                                            let abi = String::from_utf8(payload.content_abi).expect("Invalid UTF-8");
                                            let _ = match process_wasm(&payload.job_content,abi,&payload.job_id).await {
                                                Ok(res) => {
                                                    let _ = tx.send(res).await;
                                                    Ok(())
                                                }
                                                Err(e) => {
                                                    println!("ERROR: {:?}, failed to process wasm", e);
                                                    Err(e)
                                                }
                                            };
                                        }
                                        Err(e) => {
                                            eprintln!("{:?} - msg: Failed to decode payload", e);
                                        }
                                    }
                                }
                            })
                        }
                    }
                    Err(_) => {
                        println!("{:?} - msg: No pending payload in storage", StorageError::NoPendingPayloadFound);
                    }
                }
            }
            sleep_until(Instant::now() + Duration::from_millis(1000)).await;
        }
    });
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

async fn process_wasm(wasm_code : &[u8], wasm_abi : String, job_id: &[u8]) -> Result<JobResult, StorageError> {
    // let result_from_wasm = b"some result idk, idc";
    // Mock processing time
    let res = abi_reader(&wasm_abi, wasm_code)
        .await
        .expect("Shoudl work bruv");
    // sleep_until(Instant::now() + Duration::from_millis(1000)).await;
    let result = JobResult {
        job_id: job_id.to_vec(),
        result: res,
    };
    Ok(result)
}

fn key_list_parser(key_list_raw : &[u8]) -> Result<Vec<Vec<u8>>, StorageError>
{
    let key_list = &key_list_raw[4..];
    let chunks = key_list.chunks(32).map(|chunk| {
        chunk.to_vec()
    }).collect::<Vec<Vec<u8>>>();

    Ok(chunks)
}



