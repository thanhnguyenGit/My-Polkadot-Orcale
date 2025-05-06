#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{BoundedVec, CloneNoBound, DefaultNoBound, PartialEqNoBound};
use frame_support::pallet_prelude::TypeInfo;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::offchain::{StorageKind, Timestamp};
use sp_io::offchain::{timestamp, local_storage_clear, local_storage_get, local_storage_set, local_storage_compare_and_set,sleep_until};
use sp_runtime::offchain::storage_lock::{BlockAndTime, StorageLock};
use sp_io::hashing::blake2_256;

use frame_system;
pub use pallet::*;
pub mod weights;
use sp_std::vec::Vec;
use sp_std::{vec,map};
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::KeyedVec;
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*, DefaultNoBound};
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
	}
	#[pallet::storage]
	#[pallet::getter(fn get_script)]
	pub type ScriptStorage<T : Config> = StorageMap<_,Blake2_128Concat,T::AccountId,ScriptDetail<T>, OptionQuery>;
	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, DefaultNoBound,
	)]
	#[scale_info(skip_type_params(T))]
	pub struct ScriptDetail<T: Config> {
		pub(crate) block_number: BlockNumberFor<T>,
		pub(crate) wasm_code: BoundedVec<u8, T::MaxScriptSize>,
		pub(crate) hash: T::Hash,
		pub(crate) typ : Type,
		pub(crate) ref_count : u64,
	}

	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, DefaultNoBound,
	)]
	#[scale_info(skip_type_params(T))]
	pub struct OCWJobManager<T: Config> {
		block_number: BlockNumberFor<T>,
		is_status: bool,

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
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(block_number: BlockNumberFor<T>) {
			let _ = Self::simulate_heavy_computation(&block_number);
			let _ = Self::sent_meta_val(&block_number);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn upload_wasm(origin: OriginFor<T>, wasm_code: Vec<u8>,typ: Type) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				wasm_code.len().le(&(T::MaxScriptSize::get() as usize)),
				Error::<T>::SizeTooBig
        	);

			let code_hash = T::Hashing::hash(&wasm_code);

			let bounded_wasm_code = BoundedVec::try_from(wasm_code.clone()).map_err(|_| Error::<T>::SizeTooBig)?;
			let bounded_wasm_code_size = bounded_wasm_code.len() as u64;

			let script_detail = ScriptDetail {
				block_number: frame_system::Pallet::<T>::block_number(),
				typ,
				ref_count : 0,
				hash: code_hash,
				wasm_code: bounded_wasm_code,
			};

			<ScriptStorage<T>>::insert(&who, script_detail);

			Self::deposit_event(Event::ScriptUpload {
				block_number: frame_system::Pallet::<T>::block_number(),
				size: bounded_wasm_code_size,
				who,
				hash_id: code_hash,
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
}
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
struct MetaWasmStore<T : Config> {
	key_list: HashMap<Vec<Vec<u8>>, OCWState<T>>
}

impl<T : Config> From<OCWState<T>> for Vec<u8> {
	fn from(state: OCWState<T>) -> Self {
		state.encode()
	}
}

impl<T: Config> Pallet<T> {
	fn simulate_heavy_computation(block_number: &BlockNumberFor<T>) -> Result<(), &'static str> {
		let local_lock = b"wasmstore::ocw::do_heavy_computation_lock";
		let current_time = timestamp().unix_millis();
		let parent_hash = <frame_system::Pallet<T>>::block_hash(*block_number - 1u32.into());
		let value : OCWState<T> = OCWState {
			is_active: true,
			start_at_block: block_number.clone(),
			parent_hash,
			start_at_time: current_time,
		};

		let debug_value = value.clone();
		let payload : Vec<u8> = value.into();
		match local_storage_get(StorageKind::PERSISTENT, local_lock) {
			None => {
				local_storage_set(StorageKind::PERSISTENT, local_lock, &payload);
			}
			Some(_) => {
				log::error!("WASMSTORE! Cannot instantiate new OCW");
				return Err("WASMSTORE! OCW is still running")
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
	fn sent_meta_val(block_number: &BlockNumberFor<T>) -> Result<(), &'static str> {
		let local_lock = b"wasmstore::meta";
		let local_val = b"testing_meta";
		if let Some(_) = local_storage_get(StorageKind::PERSISTENT, local_lock) {
			Err("Already exist")
		} else {
			local_storage_set(StorageKind::PERSISTENT, local_lock,local_val);
			log::info!("Succesfully add key: {:?} - val: {:?} at {:#?}",local_lock,local_val,block_number);
			Ok(())
		}
	}
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

