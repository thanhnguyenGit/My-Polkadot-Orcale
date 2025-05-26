#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{BoundedVec, CloneNoBound, DefaultNoBound, PartialEqNoBound};
use frame_support::dispatch::{DispatchResult};
use frame_support::pallet_prelude::TypeInfo;
use frame_support::traits::Len;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::offchain::{StorageKind, Timestamp};
use sp_io::offchain::{timestamp, local_storage_clear, local_storage_get, local_storage_set, local_storage_compare_and_set,sleep_until};
use sp_runtime::offchain::storage_lock::{BlockAndTime, StorageLock};
use sp_io::hashing::blake2_256;
use frame_support::traits::Get;

use frame_system::{
	self as system,
	offchain::{
		AppCrypto, CreateSignedTransaction, SendSignedTransaction, SendUnsignedTransaction,
		SignedPayload, Signer, SigningTypes, SubmitTransaction, SendTransactionTypes
	},
};
pub use pallet::*;
pub mod weights;
use sp_std::vec::Vec;
use sp_std::vec;
use sp_std::collections::{
	btree_map::{BTreeMap},
	btree_set,
	vec_deque,
};
use scale_info::prelude::{
	string::String,
	format
};
use sp_runtime::traits::Clear;
use sp_runtime::transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction, TransactionPriority};
// Pallet import
use pallet_session as PalletSession;
use pallet_authorship as PalletAuthorship;
use pallet_collator_selection as PalletCollators;
use pallet_balances as PalletBalances;
// Local import
use model::wasm_compatiable::{RequestPayload,ResponePayload,JobState};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::{decode_from_bytes, decode_vec_with_len, Codec, KeyedVec};
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*, DefaultNoBound};
	use frame_support::traits::dynamic_params::IntoKey;
	use frame_system::pallet_prelude::*;
	use pallet_collator_selection::CandidateList;
	use sp_core::Public;
	use sp_io::offchain::timestamp;
	use sp_runtime::traits::{CheckedAdd, One, Hash};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_collator_selection::Config + pallet_balances::Config + pallet_authorship::Config + pallet_session::Config + CreateSignedTransaction<Call<Self>>  {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type WeightInfo: crate::weights::WeightInfo;
		// type AuthorityId : Member + Codec + Public;
		#[pallet::constant]
		type MaxScriptSize : Get<u32>;

		#[pallet::constant]
		type MaxScriptStorageCap : Get<u32>;

		#[pallet::constant]
		type MaxScriptKeyLen : Get<u32>;
		#[pallet::constant]
		type MaxStringSize : Get<u32>;
		#[pallet::constant]
		type MaxResultSize : Get<u32>;
		#[pallet::constant]
		type MaxResultSubmition : Get<u32>;
		#[pallet::constant]
		type MaxJobs : Get<u32>;
		#[pallet::constant]
		type UnsignedPriority: Get<TransactionPriority>;

	}
	#[pallet::storage]
	#[pallet::getter(fn get_script)]
	pub type ScriptStorage<T : Config> = StorageMap<_,Blake2_128Concat,BoundedVec<u8,T::MaxScriptKeyLen>,ScriptDetail<T>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn get_script_result)]
	pub type ScriptResult<T : Config> = StorageMap<_,Blake2_128Concat,T::Hash,BoundedVec<SignedScriptResult<T>,T::MaxResultSubmition>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn get_jobs)]
	pub type JobQueue<T : Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::Hash,
		OCWJob<T>,
		OptionQuery
	>;

	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound,Default
	)]
	#[scale_info(skip_type_params(T))]
	pub struct ScriptDetail<T: Config> {
		publisher: T::AccountId,
		block_number: BlockNumberFor<T>,
		pub(crate) wasm_code: BoundedVec<u8, T::MaxScriptSize>,
		hash: T::Hash,
		name: BoundedVec<u8,T::MaxStringSize>,
		typ : Type,
		ref_count : u64,
	}

	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, Default
	)]
	#[scale_info(skip_type_params(T))]
	pub struct OCWJob<T: Config> {
		pub(crate) caller : T::AccountId,
		pub(crate) script_name : BoundedVec<u8,T::MaxStringSize>,
		pub(crate) wasm_code: BoundedVec<u8, T::MaxScriptSize>,
		pub(crate) state: JobState,
	}

	#[derive(
		Encode, Decode,MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, Default
	)]
	#[scale_info(skip_type_params(T))]
	pub struct SignedScriptResult<T: Config> {
		// pub(crate) signature : T::Signature,
		pub(crate) result_data : BoundedVec<u8,T::MaxResultSize>,
		// pub(crate) public : T::AccountId,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ScriptUpload {
			who: T::AccountId,
			block_number: BlockNumberFor<T>,
			size : u64,
			script_name : String,
			hash_id : T::Hash,
		},
		RequestExecution {
			job_id : T::Hash,
			who: T::AccountId,
			block_number: BlockNumberFor<T>,
			script_name: String
		},
		UnsignedTx {
			when: BlockNumberFor<T>
		},
		Test {
			key : Vec<T::AccountId>,

		}
	}

	#[derive(Encode,MaxEncodedLen,Default, Decode, Clone, PartialEq, Eq, RuntimeDebug, scale_info::TypeInfo)]
	pub enum Type {
		#[default]
		Executable,
		Source
	}

	#[pallet::error]
	pub enum Error<T> {
		SizeTooBig,
		HashCheckFail,
		InvalidWasmFormat,
		NotOwner,
		ValAlreadyExist,
		NoValExist,
		MaxCapReach,
		InvalidOperation,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(_block_number: BlockNumberFor<T>) {
			let master_key = b"wasmstore_jobs_executor";
			let mastter_result_key = b"wasmstore_jobs_result";

			let job_key = local_storage_get(StorageKind::PERSISTENT, master_key).expect("Value uninitialize");
			let result_key = local_storage_get(StorageKind::PERSISTENT, mastter_result_key).expect("Value uninitialize");

			match Self::ocw_do_handling_job(&job_key).ok() {
				None => {
					log::error!("No payload returned");
				}
				Some(command) => {
					match command {
						Command::None => {
							log::info!("Waiting for Pending");
							return;
						}
						Command::RequestData { key_list, data } => {
							log::info!("New value: {:?}",data);
							log::info!("Key list: {:?}",key_list);
							let mut encoded_payload = data;
							match RequestPayload::decode(&mut &encoded_payload[..]) {
								Ok(payload) => {
									log::info!("Decoded payload successful {:?}", payload.job_id);
									local_storage_compare_and_set(StorageKind::PERSISTENT, master_key, Some(job_key), &key_list);
									local_storage_set(StorageKind::PERSISTENT, &payload.job_id, &encoded_payload);
								}
								Err(e) => {
									log::error!("Failed to decode: {:?}", e);
								}
							};
						}
						Command::ResultData {result_list} => {
							for key in result_list.iter() {
								match local_storage_get(StorageKind::PERSISTENT, &key) {
									None => {}
									Some(val) => {
										// let _ = Self::ocw_do_submit_result(key, val);
										log::info!("Found finished result: {:08x?}", key);
									}
								}
							}
						}
					}
				}
			}
			match Self::ocw_do_handling_result(&result_key) {
				Ok(result_key_list) => {
					for result_key in result_key_list.iter() {
						log::info!("Result list len: {:?}", result_key_list.len());
						match local_storage_get(StorageKind::PERSISTENT, result_key) {
							None => {}
							Some(raw_val) => {
								match ResponePayload::decode(&mut &raw_val[..]) {
									Ok(res) => {
										log::info!("Finished Result: K - {:?}, V - {:?}", result_key, res);
									}
									Err(e) => {
										log::error!("ERROR: {:?} - msg Failed to decode response",e)
									}
								}
							}
						}

					}
				}
				Err(e) => {}
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn upload_wasm(origin: OriginFor<T>, mut name : String, typ: Type, wasm_code: Vec<u8>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			name = match typ {
				Type::Executable => format!("EXE{}", name.as_str()),
				Type::Source => format!("SRC{}", name.as_str())
			};
			let name_clone = name.clone();
			let code_hash = T::Hashing::hash(&wasm_code);

			ensure!(
				wasm_code.len().le(&(T::MaxScriptSize::get() as usize)),
				Error::<T>::SizeTooBig
        	);

			let bounded_wasm_code = BoundedVec::try_from(wasm_code.clone()).map_err(|_| Error::<T>::SizeTooBig)?;
			let bounded_name = BoundedVec::try_from(name.into_bytes()).map_err(|_|Error::<T>::SizeTooBig)?;
			let bounded_wasm_code_size = bounded_wasm_code.len() as u64;

			let script_key = {
				let key = format!("{:?}",name_clone.as_str()).encode();
				BoundedVec::try_from(key).map_err(|_|Error::<T>::SizeTooBig)?
			};
			let val = ScriptStorage::<T>::get(&script_key);
			match val {
				None => {}
				Some(script_detail) => {
					ensure!(
						bounded_name.ne(&script_detail.name) || bounded_wasm_code.ne(&script_detail.wasm_code)
						,Error::<T>::ValAlreadyExist
					)
				}
			}

			let script_detail = ScriptDetail {
				publisher: who.clone(),
				name: bounded_name,
				block_number: frame_system::Pallet::<T>::block_number(),
				typ,
				ref_count : 0,
				hash: code_hash,
				wasm_code: bounded_wasm_code,
			};

			<ScriptStorage<T>>::insert(&script_key, script_detail);

			Self::deposit_event(Event::<T>::ScriptUpload {
				block_number: frame_system::Pallet::<T>::block_number(),
				size: bounded_wasm_code_size,
				who,
				script_name: name_clone,
				hash_id: code_hash,
			});
			Ok(())
		}
		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn request_script_execution(origin: OriginFor<T>, script_name : String) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let script_key = {
				let key = format!("{:?}",script_name.as_str()).encode();
				BoundedVec::try_from(key).map_err(|_|Error::<T>::SizeTooBig)?
			};
			let val = <ScriptStorage<T>>::get(&script_key);
			let mut job_id = T::Hashing::hash(&[0u8;32]);
			match val {
				None => {
					return Err(Error::<T>::NoValExist.into());
				}
				Some(val) => {
					let name = BoundedVec::try_from(script_name.clone().into_bytes()).map_err(|_|Error::<T>::SizeTooBig)?;
					// A caller cannot call a script after it being called and is being processed.
					// Hence use caller + scrip name as hash.
					let job = OCWJob {
						caller: who.clone(),
						script_name: name,
						wasm_code: val.wasm_code,
						state: JobState::Pending,
					};
					let script_key_hash = T::Hashing::hash(&job.encode());
					job_id = T::Hashing::hash(&job.encode());
					match JobQueue::<T>::get(&script_key_hash) {
						None => {JobQueue::<T>::insert(script_key_hash, job)}
						Some(_) => {
							log::error!("ERROR: Duplicate Job - msg: Job {:?} already exist",script_key_hash);
							return Err(Error::<T>::ValAlreadyExist.into());
						}
					}

				}
			}
			Self::deposit_event(Event::<T>::RequestExecution {
				job_id,
				who,
				block_number: frame_system::Pallet::<T>::block_number(),
				script_name
			});
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn change_job_state(origin: OriginFor<T>,block_number: BlockNumberFor<T>,key: T::Hash) -> DispatchResult {
			ensure_none(origin)?;
			JobQueue::<T>::mutate(key, |val| {
				if let Some(existing_job) = val {
					match existing_job.state {
						JobState::Pending => {existing_job.state = JobState::Processing}
						JobState::Processing => {existing_job.state = JobState::Finish}
						_ => {}
					}
				}
			});
			Ok(())
		}
		#[pallet::call_index(3)]
		#[pallet::weight(0)]
		pub fn submit_result_unsiged(origin: OriginFor<T>, block_number: BlockNumberFor<T>, key: T::Hash, value : Vec<u8>) -> DispatchResult {
			ensure_none(origin)?;

			if let Some(val) = JobQueue::<T>::get(key) {
				if val.state != JobState::Finish {
					return Err(Error::<T>::InvalidOperation.into());
				}
				let payload = SignedScriptResult {
					// signature: (),
					result_data: BoundedVec::try_from(value).map_err(|_| Error::<T>::SizeTooBig)?,
					// public: (),
				};
				match ScriptResult::<T>::get(key) {
					None => {
						let mut list : BoundedVec<SignedScriptResult<T>,T::MaxResultSubmition> = BoundedVec::new();
						let _ = list.try_push(payload);
						ScriptResult::<T>::insert(key, list);
					}
					Some(existing_list) => {
						let mut list : BoundedVec<SignedScriptResult<T>,T::MaxResultSubmition>= existing_list;
						let _ = list.try_push(payload);
						ScriptResult::<T>::insert(&key, list);
					}
				}
			}
			Ok(())
		}
		#[pallet::call_index(4)]
		#[pallet::weight(0)]
		pub fn get_collators_lts(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let maybe_author = PalletAuthorship::Pallet::<T>::author();
			let acc = PalletCollators::Pallet::<T>::account_id();
			log::info!("Acc {:?}", acc);
			let mut result = Vec::new();
			let collators = PalletCollators::pallet::Invulnerables::<T>::get();
			for collator in collators.iter() {

				result.push(collator.clone());
			}
			Self::deposit_event(Event::<T>::Test {
				key: result,
			});
			Ok(())
		}
	}
	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::change_job_state {block_number,key} = call {
				Self::validate_unsiged_transaction(block_number, key)
			} else if let Call::submit_result_unsiged { block_number,key, .. } = call {
				Self::validate_unsiged_transaction(block_number, key)
			}
			else {
				InvalidTransaction::Call.into()
			}

		}
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
struct OCWState<T : Config > {
	is_active: bool,
	start_at_block: BlockNumberFor<T>,
	parent_hash: T::Hash,
	start_at_time: u64,
	script: Vec<u8>,
}
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
pub enum Command {
	None,
	RequestData {
		key_list : Vec<u8>,
		data : Vec<u8>
	},
	ResultData {
		result_list : Vec<Vec<u8>>,
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
enum WasmStoreErr {
	ValueAlreadyExist,
	IsStillRunning,
	Unintialize,
	Other,
}

impl<T: Config> Pallet<T> {
	fn ocw_do_change_job_state(key: T::Hash) -> Result<(), &'static str> {
		let block_number = system::Pallet::<T>::block_number();
		let call = Call::change_job_state {block_number,key };
		SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
			.map_err(|()| "Unable to submit unsigned transaction.")?;
		Self::deposit_event(Event::<T>::UnsignedTx {
			when: block_number
		});
		Ok(())
	}

	fn ocw_do_submit_result(key : T::Hash, value : Vec<u8>) -> Result<(), &'static str> {
		let block_number = system::Pallet::<T>::block_number();
		let call = Call::submit_result_unsiged {block_number,key, value};
		SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
			.map_err(|()| "Unable to submit unsigned transaction.")?;
		Self::deposit_event(Event::<T>::UnsignedTx {
			when: block_number
		});
		Ok(())
	}
	fn simulate_heavy_computation(block_number: &BlockNumberFor<T>) -> Result<(),WasmStoreErr> {
		let local_lock = b"wasmstore::ocw::do_heavy_computation_lock";
		let current_time = timestamp().unix_millis();
		let parent_hash = <frame_system::Pallet<T>>::block_hash(*block_number - 1u32.into());
		let value : OCWState<T> = OCWState {
			is_active: true,
			start_at_block: block_number.clone(),
			parent_hash,
			start_at_time: current_time,
			script: Vec::new(),
		};

		let debug_value = value.clone();
		let payload : Vec<u8> = value.encode();
		match local_storage_get(StorageKind::PERSISTENT, local_lock) {
			None => {
				local_storage_set(StorageKind::PERSISTENT, local_lock, &payload);
			}
			Some(_) => {
				log::error!("WASMSTORE! Cannot instantiate new OCW");
				return Err(WasmStoreErr::IsStillRunning)
			}
		}

		log::info!("WASMSTORE! OCW is running at block: {:?} (parent hash: {:#?}), start at: {:#?},  STATUS: {:#?}", debug_value.start_at_block, debug_value.parent_hash,debug_value.start_at_time,debug_value.is_active);

		let mut data = vec![0u8; 1024];
		let iterations = 20_000_000;

		log::info!("Starting heavy OCW task...");
		let mut value = 0;
		for i in 0..iterations {
			data[0] = (i % 256) as u8;

			let _ = blake2_256(&data);
			if i % 100_000 == 0 {
				value += i;
			}
		}

		log::info!("Finished heavy OCW task after {} iterations, result: {}, clearing storage for furure instances", iterations,value);
		local_storage_clear(StorageKind::PERSISTENT, local_lock);
		Ok(())
	}

	// Need to have somekind of guard due to this function interact with localstorage
	// Extrensic cannot do that, only OCW can, future macro attribute design for this
	fn add_wasm_script_to_storage(name: String,script: &[u8],block_number: BlockNumberFor<T>) -> Result<(),WasmStoreErr> {
		let current_time = timestamp().unix_millis();
		let local_lock = format!("{:?}{:?}{:?}",name, block_number, current_time).into_bytes();
		let parent_hash = <frame_system::Pallet<T>>::block_hash(block_number - 1u32.into());
		let value : OCWState<T> = OCWState {
			is_active: false,
			start_at_block: block_number.clone(),
			parent_hash,
			start_at_time: current_time,
			script : script.to_vec(),
		};
		let payload : Vec<u8> = value.encode();
		match local_storage_get(StorageKind::PERSISTENT,&local_lock) {
			None => {
				local_storage_set(StorageKind::PERSISTENT, &local_lock, &payload);
				Ok(())
			}
			Some(_) => {
				Err(WasmStoreErr::ValueAlreadyExist)
			}
		}

	}


	fn add_to_key_list(key: &[u8],index : usize, key_len : u32) {
		match local_storage_get(StorageKind::PERSISTENT,b"wasmstore::keylist") {
			None => {}
			Some(val) => {
				let mut new_val = val.clone();
				let mut new_key = format!("{}{:?}",key_len,key).into_bytes();
				new_val.append(&mut new_key);
				local_storage_compare_and_set(StorageKind::PERSISTENT, b"wasmstore::keylist", Some(val), &new_val);
			}
		}
	}

	fn ocw_do_handling_job(params: &[u8]) -> Result<Command, WasmStoreErr> {
		log::info!("WASMSTORE: job handler running. Local value is {:?}", params);
		let mut response = RequestPayload::default();
		for (job_id, mut job) in JobQueue::<T>::iter() {
			return match job.state {
				JobState::Pending => {
					log::info!("Found pending job");

					let mut key_list = params.to_vec();
					key_list.extend_from_slice(&job_id.encode());
					log::info!("Job id: {:?}", job_id);
					log::info!("Job id SCALE-encoded {:?}",job_id.encode());
					log::info!("Key list: {:?}", key_list);
					match Self::ocw_do_change_job_state(job_id) {
						Ok(_) => {
							log::info!("WASMSTORE! OCW job: caller - {:?}, script_name: {:?}, state: {:?}",job.caller, job.script_name, job.state);
							response = RequestPayload {
								job_id : job_id.encode(),
								job_content: job.wasm_code.to_vec(),
								job_state: job.state,
							};
							log::info!("Payload detail: job_id - {:?}, state: {:?}", response.job_id,response.job_state);
						}
						Err(e) => {log::error!("ERROR: Value not exist, msg: {}", e)}
					}
					Ok(Command::RequestData {
						key_list,
						data : response.encode()
					})
				}
				JobState::Finish => {
					log::info!("Found finished job");
					let mut result_key_list = Vec::new();
					result_key_list.push(job_id.encode());
					Ok(Command::ResultData {
						result_list: result_key_list,
					})
				}
				_ => {
					continue
				}
			}
		}
		Ok(Command::None)
	}

	fn ocw_do_handling_result(params : &[u8]) -> Result<Vec<Vec<u8>>, WasmStoreErr>{
		if params == b"init" {
			log::error!("No result yet");
			return Err(WasmStoreErr::IsStillRunning)
		}
		let data = &params[4..];
		let key_list = data.chunks(32).map(|chunk| {
			chunk.to_vec()
		}).collect::<Vec<Vec<u8>>>();
		Ok(key_list)
	}

	fn validate_unsiged_transaction(block_number : &BlockNumberFor<T>,key: &T::Hash) -> TransactionValidity {
		let current_block = system::Pallet::<T>::block_number();
		if current_block < *block_number {
			log::warn!("Unsiged Transaction period corrupted");
			log::warn!("block number: {:#?}",block_number);
			return InvalidTransaction::Future.into();
		}
		if key.is_clear() {
			return InvalidTransaction::BadProof.into();
		}
		ValidTransaction::with_tag_prefix("WasmstoreJobWorker")
			.priority(T::UnsignedPriority::get())
			.longevity(5)
			.propagate(true)
			.build()
	}
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;
