#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[frame_support::pallet]
pub mod pallet {
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*, DefaultNoBound};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{CheckedAdd, One};


	#[pallet::config]
	pub trait Config: frame_system::Config {

		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;


		type WeightInfo: crate::weights::WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, DefaultNoBound,
	)]
	#[scale_info(skip_type_params(T))]
	pub struct CompositeStruct<T: Config> {
		/// A block number.
		pub(crate) block_number: BlockNumberFor<T>,
	}


	#[pallet::storage]
	pub type Something<T: Config> = StorageValue<_, CompositeStruct<T>>;


	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {

		SomethingStored { block_number: BlockNumberFor<T>, who: T::AccountId },
	}


	#[pallet::error]
	pub enum Error<T> {

		NoneValue,

		StorageOverflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {

		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn do_something(origin: OriginFor<T>, bn: u32) -> DispatchResultWithPostInfo {
			// Check that the extrinsic was signed and get the signer.
			// This function will return an error if the extrinsic is not signed.
			// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/frame_origin/index.html>
			let who = ensure_signed(origin)?;

			// Convert the u32 into a block number. This is possible because the set of trait bounds
			// defined in [`frame_system::Config::BlockNumber`].
			let block_number: BlockNumberFor<T> = bn.into();

			// Update storage.
			<Something<T>>::put(CompositeStruct { block_number });

			// Emit an event.
			Self::deposit_event(Event::SomethingStored { block_number, who });

			// Return a successful [`DispatchResultWithPostInfo`] or [`DispatchResult`].
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
		pub fn cause_error(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			// Read a value from storage.
			match <Something<T>>::get() {
				// Return an error if the value has not been set.
				None => Err(Error::<T>::NoneValue)?,
				Some(mut old) => {
					// Increment the value read from storage; will error in the event of overflow.
					old.block_number = old
						.block_number
						.checked_add(&One::one())
						// ^^ equivalent is to:
						// .checked_add(&1u32.into())
						// both of which build a `One` instance for the type `BlockNumber`.
						.ok_or(Error::<T>::StorageOverflow)?;
					// Update the value in storage with the incremented result.
					<Something<T>>::put(old);
					// Explore how you can rewrite this using
					// [`frame_support::storage::StorageValue::mutate`].
					Ok(().into())
				},
			}
		}
	}
}
