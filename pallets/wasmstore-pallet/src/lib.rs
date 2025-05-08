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

use frame_system;
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
	pub trait Config: frame_system::Config  {
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
	#[pallet::getter(fn get_script_key_list)]
	pub type ScriptKeyList<T : Config> = StorageValue<_,BoundedVec<BoundedVec<u8,T::MaxScriptKeyLen>,T::MaxScriptStorageCap>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn get_jobs)]
	pub type JobManager<T : Config> = StorageMap<_, Blake2_128Concat, T::AccountId,BoundedVec<OCWJob<T>,T::MaxJobs>, OptionQuery>;
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
		result: BoundedVec<u8,T::MaxStringSize>,
	}

	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, DefaultNoBound,
	)]
	#[scale_info(skip_type_params(T))]
	pub struct OCWJob<T: Config> {
		script_name : BoundedVec<u8,T::MaxStringSize>,
		state: JobState,
	}

	#[derive(Encode,MaxEncodedLen,Default, Decode, Clone, PartialEq, Eq, RuntimeDebug, scale_info::TypeInfo)]
	enum JobState {
		#[default]
		Idle,
		Processing,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ScriptUpload {
			who: T::AccountId,
			block_number: BlockNumberFor<T>,
			size : u64,
			hash_id : T::Hash,
		},
		RequestExecution {
			who: T::AccountId,
			block_number: BlockNumberFor<T>,
			script_name: String
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
		ValNoneExist,
		MaxCapReach,

	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(block_number: BlockNumberFor<T>) {
			// let _ = Self::simulate_heavy_computation(&block_number);
			// let _ = Self::sent_meta_val(&block_number);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn upload_wasm(origin: OriginFor<T>, mut name : String, typ: Type, wasm_code: Vec<u8>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			match typ {
				Type::Executable => {name = format!("EXE{:?}", name)}
				Type::Source => {name = format!("SRC{:?}", name)}
			}
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
				let key = format!("{:?}",name_clone).encode();
				BoundedVec::try_from(key).map_err(|_|Error::<T>::SizeTooBig)?
			};
			let val = ScriptStorage::<T>::get(&script_key);
			match val {
				None => {}
				Some(script_detail) => {
					ensure!(
						bounded_name.eq(&script_detail.name) || bounded_wasm_code.eq(&script_detail.wasm_code)
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
				result: BoundedVec::new(),
			};

			<ScriptStorage<T>>::insert(&script_key, script_detail);
			// Self::add_key_to_keylist(script_key).expect("TODO: panic message");

			Self::deposit_event(Event::<T>::ScriptUpload {
				block_number: frame_system::Pallet::<T>::block_number(),
				size: bounded_wasm_code_size,
				who,
				hash_id: code_hash,
			});
			Ok(())
		}
		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn request_script_execution(origin: OriginFor<T>, script_name : String) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let script_key = {
				let key = format!("{:?}",script_name).encode();
				BoundedVec::try_from(key).map_err(|_|Error::<T>::SizeTooBig)?
			};
			let val = <ScriptStorage<T>>::get(&script_key);
			match val {
				None => {
					return Err(Error::<T>::ValNoneExist.into());
				}
				Some(_) => {
					let name = BoundedVec::try_from(script_name.clone().into_bytes()).map_err(|_|Error::<T>::SizeTooBig)?;

					let job = OCWJob {
						script_name: name,
						state: JobState::Processing,
					};
					match JobManager::<T>::get(&who) {
						None => {
							let mut jobs = BoundedVec::<_, T::MaxJobs>::new();
							let _ = jobs.try_push(job);
							JobManager::<T>::insert(who.clone(), jobs);
						}
						Some(_) => {
							JobManager::<T>::mutate(&who, |val| {
								if let Some(ref mut jobs) = val {
									let _ = jobs.try_push(job);
								}
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

enum WasmStoreErr {
	ValueAlreadyExist,
	IsStillRunning,
}

impl<T: Config> Pallet<T> {
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

	fn add_key_to_localstorage_key_list() -> Result<(),Error<T>> {
		let key_list = ScriptKeyList::<T>::get();
		for (i,val) in key_list.iter().enumerate() {
			if val.len() > u32::MAX as usize {
				return Err(Error::<T>::SizeTooBig);
			}
			Self::add_to_key_list(val, i, val.len() as u32);
		}
		Ok(())
	}

	fn add_key_to_keylist(key: BoundedVec<u8, T::MaxScriptKeyLen>) -> DispatchResult {
		ScriptKeyList::<T>::try_mutate(|list| {
			list.try_push(key).map_err(|_| Error::<T>::SizeTooBig)
		})?;

		Ok(())
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
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

