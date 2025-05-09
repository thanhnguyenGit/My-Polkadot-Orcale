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
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::KeyedVec;
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*, DefaultNoBound};
	use frame_support::traits::dynamic_params::IntoKey;
	use frame_system::pallet_prelude::*;
	use sp_io::offchain::timestamp;
	use sp_runtime::traits::{CheckedAdd, One, Hash};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + CreateSignedTransaction<Call<Self>>  {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type WeightInfo: crate::weights::WeightInfo;
		#[pallet::constant]
		type MaxScriptSize : Get<u32>;

		#[pallet::constant]
		type MaxScriptStorageCap : Get<u32>;

		#[pallet::constant]
		type MaxScriptKeyLen : Get<u32>;
		#[pallet::constant]
		type MaxStringSize : Get<u32>;
		#[pallet::constant]
		type MaxJobs : Get<u32>;
	}
	#[pallet::storage]
	#[pallet::getter(fn get_script)]
	pub type ScriptStorage<T : Config> = StorageMap<_,Blake2_128Concat,BoundedVec<u8,T::MaxScriptKeyLen>,ScriptDetail<T>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn get_script_result)]
	pub type ScriptResult<T : Config> = StorageMap<_,Blake2_128Concat,BoundedVec<u8,T::MaxScriptKeyLen>,BoundedVec<u8,T::MaxScriptKeyLen>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn get_jobs)]
	pub type JobQueue<T : Config> = StorageValue<_,BoundedVec<OCWJob<T>,T::MaxJobs>, ValueQuery>;
	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound,Default
	)]
	#[scale_info(skip_type_params(T))]
	pub struct ScriptDetail<T: Config> {
		publisher: T::AccountId,
		block_number: BlockNumberFor<T>,
		wasm_code: BoundedVec<u8, T::MaxScriptSize>,
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
		pub(crate) state: JobState,
	}

	#[derive(Encode,MaxEncodedLen,Default, Decode, Clone, PartialEq, Eq, RuntimeDebug, scale_info::TypeInfo)]
	pub enum JobState {
		#[default]
		Idling,
		Pending,
		Processing,
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
			who: T::AccountId,
			block_number: BlockNumberFor<T>,
			script_name: String
		},
		UnsignedTx {
			when: BlockNumberFor<T>
		},
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

	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(block_number: BlockNumberFor<T>) {
			// let _ = Self::simulate_heavy_computation(&block_number);
			// let _ = Self::sent_meta_val(&block_number);
			let _ = Self::ocw_do_handling_job(block_number);
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
			match val {
				None => {
					return Err(Error::<T>::NoValExist.into());
				}
				Some(_) => {
					let name = BoundedVec::try_from(script_name.clone().into_bytes()).map_err(|_|Error::<T>::SizeTooBig)?;

					let job = OCWJob {
						caller: who.clone(),
						script_name: name,
						state: JobState::Pending,
					};
					match JobQueue::<T>::try_get() {
						Err(_) => {
							let mut jobs = BoundedVec::<_, T::MaxJobs>::new();
							let _ = jobs.try_push(job);
							JobQueue::<T>::set(jobs);
						}
						Ok(_) => {
							JobQueue::<T>::mutate( |jobs| {
								let _ = jobs.try_push(job);
							});
						}
                    }
				}
			}
			Self::deposit_event(Event::<T>::RequestExecution {
				who,
				block_number: frame_system::Pallet::<T>::block_number(),
				script_name
			});
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn change_job_state(origin: OriginFor<T>) -> DispatchResult {
			ensure_none(origin)?;
			Self::handling_job().expect("Error at job handling");
			Ok(())
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

enum State {
	Idle,
	Pending
}
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
struct MetaWasmStore {
	key_list: BTreeMap<Vec<u8>, Vec<Vec<u8>>>
}

impl<T : Config> From<OCWState<T>> for Vec<u8> {
	fn from(state: OCWState<T>) -> Self {
		state.encode()
	}
}

impl MetaWasmStore {
	fn add_to_list(&mut self, key: &[u8],val: &[u8]){
		match self.key_list.get(key) {
			None => {
				let mut key_list = Vec::new();
				key_list.push(val.to_vec());
				self.key_list.insert(key.to_vec(),key_list);
			}
			Some(_) => {}
		}
	}
	fn update_key_list(&mut self, key: &[u8], new_val:&[u8]){
		match self.key_list.get_mut(key) {
			None => {}
			Some(val) => {
				val.push(new_val.to_vec());
			}
		}
	}
	fn remove_key_from_key_list(&mut self, key: &[u8], val_to_remove:&[u8]){
		match self.key_list.get_mut(key) {
			None => {}
			Some(val) => {
				val.retain(|ele| *ele != val_to_remove.to_vec());
			}
		}
	}
	fn delete_key(&mut self, key: &[u8]){
		match self.key_list.get(key) {
			None => {}
			Some(_) => {
				self.key_list.remove_entry(key);
			}
		}
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
enum WasmStoreErr {
	ValueAlreadyExist,
	IsStillRunning,
	Unintialize,
	// Other(&'a str)
}

// impl From<&str> for WasmStoreErr {
// 	fn from(value: &str) -> Self {
// 		WasmStoreErr::Other(value)
// 	}
// }

impl<T: Config> Pallet<T> {
	fn ocw_do_handling_job(block_number: BlockNumberFor<T>) -> Result<(), &'static str>{
		let call = Call::change_job_state {};
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

	fn handling_job() -> Result<(),WasmStoreErr> {
		match JobQueue::<T>::exists() {
			true => {}
			false => { return Err(WasmStoreErr::Unintialize) }
		}
		JobQueue::<T>::try_mutate(|val| {
			Ok(for (i, ele) in val.iter_mut().enumerate() {
				let local_key = b"wasmstore_jobs_executor";
				match ele.state {
					JobState::Pending => {
						let mut local_val = local_storage_get(StorageKind::PERSISTENT, local_key).expect("Value for storage must be initialize");
						let len = ele.encode().len() as u32;
						let mut payload = Vec::with_capacity(4 + ele.encode().len());
						payload.extend_from_slice(&len.to_le_bytes());
						payload.extend_from_slice(&ele.encode());
						local_val.extend_from_slice(&payload);
						ele.state = JobState::Processing;
						log::info!("WASMSTORE! OCW job: caller - {:?}, script_name: {:?}, state: {:?}", ele.caller,ele.script_name,ele.state);
					}
					JobState::Processing => {
						log::info!("WASMSTORE! OCW job: caller - {:?}, script_name: {:?}, state: {:?}", ele.caller,ele.script_name,ele.state);
					}
					JobState::Idling => {
						log::info!("WASMSTORE! OCW job: caller - {:?}, script_name: {:?}, state: {:?}", ele.caller,ele.script_name,ele.state);
					}
				}
			})
		}
		)
	}
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

