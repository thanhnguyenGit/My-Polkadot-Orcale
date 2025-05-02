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
use sp_runtime::traits::Lazy;
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
			log::info!("Hello from offchain workers of WASMSTORE!");
			let parent_hash = <frame_system::Pallet<T>>::block_hash(block_number - 1u32.into());
			let _ = Self::simulate_heavy_computation(&block_number);
			log::info!("WASMSTORE! Current block: {:?} (parent hash: {:?})", block_number, parent_hash);
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

struct OCWstate {
	id: u32,
	is_active : bool
}

#[derive(Default)]
struct OCWManager {
	storage: HashMap<u64, OCWstate>
}

// impl OCWManager {
// 	fn add_ocw(&mut self, block_number : u64, )
// }

static OCW_MANAGER: Mutex<OCWManager> = Mutex::new(OCWManager::default());



impl<T: Config> Pallet<T> {
	fn simulate_heavy_computation(block_number: &BlockNumberFor<T>) -> Result<(), &'static str> {
		let parent_hash = <frame_system::Pallet<T>>::block_hash(*block_number - 1u32.into());
		log::info!("WASMSTORE! OCW current run at Current block: {:?} (parent hash: {:?})", block_number, parent_hash);

		let mut data = vec![0u8; 1024];
		let iterations = 50_000_000;

		log::info!("Starting heavy OCW task...");

		for i in 0..iterations {
			data[0] = (i % 256) as u8;

			let _ = blake2_256(&data);
			if i % 100_000 == 0 {
			}
		}

		log::info!("Finished heavy OCW task after {} iterations", iterations);
		Ok(())
	}
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

