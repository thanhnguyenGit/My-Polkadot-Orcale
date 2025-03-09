// This file is part of 'custom-pallet'.

// SPDX-License-Identifier: MIT-0

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub use frame_support::{
    dispatch::{DispatchResult,DispatchResultWithPostInfo},
    ensure,
    traits::{Get,EnsureOrigin},
};

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use super::*;
    use frame_support::dispatch::RawOrigin;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use frame_support::sp_runtime::generic::BlockId::Number;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub initial_value: u32,
        pub _marker: PhantomData<T>,
    }

    // Configuration trait for the pallet
    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type CounterMaxValue: Get<u32>;
        #[pallet::constant]
        type DefaultValue: Get<u32>;
        /// A type representing the weights required by the dispatchables of this pallet.
        type WeightInfo: WeightInfo;
    }

    /// Storage for the current value of the counter.
    #[pallet::storage]
    pub type CounterValue<T> = StorageValue<_, u32>;

    #[pallet::storage]
    #[pallet::getter(fn initial_value)]
    pub type NumberStorage<T: Config> = StorageValue<_,u32,ValueQuery>;

    /// Storage map to track the number of interactions performed by each account.
    #[pallet::storage]
    pub type UserInteractions<T: Config> = StorageMap<_, Twox64Concat, T::AccountId, u32>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CounterValueSet {
            counter_value: u32,
        },
        CounterIncremented {
            counter_value: u32,
            who: T::AccountId,
            incremented_amount: u32,
        },
        CounterDecremented {
            /// The new value set.
            counter_value: u32,
            who: T::AccountId,
            decremented_amount: u32,
        },
        DefaultValue {
            default_value: u32,
            who: T::AccountId,
        }
    }

    #[pallet::error]
    pub enum Error<T> {
        CounterValueExceedsMax,
        CounterValueBelowZero,
        CounterOverflow,
        UserInteractionOverflow,
        NotSudo,
    }

    //#[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { initial_value: T::DefaultValue::get(), _marker: Default::default() }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            NumberStorage::<T>::put(self.initial_value);
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
            if NumberStorage::<T>::get() == 0 {
                NumberStorage::<T>::put(T::DefaultValue::get())
            }
            T::DbWeight::get().writes(1)
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_counter_value())]
        pub fn set_counter_value(origin: OriginFor<T>, new_value: u32) -> DispatchResult {
            ensure_root(origin)?;

            ensure!(
                new_value <= T::CounterMaxValue::get(),
                Error::<T>::CounterValueExceedsMax
            );

            CounterValue::<T>::put(new_value);

            Self::deposit_event(Event::<T>::CounterValueSet {
                counter_value: new_value,
            });

            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::increment())]
        pub fn increment(origin: OriginFor<T>, amount_to_increment: u32) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let current_value = CounterValue::<T>::get().unwrap_or(0);

            let new_value = current_value
                .checked_add(amount_to_increment)
                .ok_or(Error::<T>::CounterOverflow)?;

            ensure!(
                new_value <= T::CounterMaxValue::get(),
                Error::<T>::CounterValueExceedsMax
            );

            CounterValue::<T>::put(new_value);

            UserInteractions::<T>::try_mutate(&who, |interactions| -> Result<_, Error<T>> {
                let new_interactions = interactions
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(Error::<T>::UserInteractionOverflow)?;
                *interactions = Some(new_interactions); // Store the new value

                Ok(())
            })?;

            Self::deposit_event(Event::<T>::CounterIncremented {
                counter_value: new_value,
                who,
                incremented_amount: amount_to_increment,
            });

            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::decrement())]
        pub fn decrement(origin: OriginFor<T>, amount_to_decrement: u32) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let current_value = CounterValue::<T>::get().unwrap_or(0);

            let new_value = current_value
                .checked_sub(amount_to_decrement)
                .ok_or(Error::<T>::CounterValueBelowZero)?;

            CounterValue::<T>::put(new_value);

            UserInteractions::<T>::try_mutate(&who, |interactions| -> Result<_, Error<T>> {
                let new_interactions = interactions
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(Error::<T>::UserInteractionOverflow)?;
                *interactions = Some(new_interactions); // Store the new value

                Ok(())
            })?;

            Self::deposit_event(Event::<T>::CounterDecremented {
                counter_value: new_value,
                who,
                decremented_amount: amount_to_decrement,
            });

            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(0)]
        pub fn check_is_sudo_privilege(origin: OriginFor<T>) -> DispatchResult {
            let who = match origin.clone().into() {
                Ok(RawOrigin::Root) => {
                    //log::info!("origin have root access");
                    return Ok(());
                }
                Ok(RawOrigin::Signed(signer)) => {
                    //log::info!("origin is singed");
                        return Ok(())
                }
                _ => Err(Error::<T>::NotSudo)?
            };
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(0)]
        pub fn grant_sudo_privilege(origin: OriginFor<T>) -> DispatchResult {

            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(0)]
        pub fn fetch_data_from_rpc_call(origin: OriginFor<T>) -> DispatchResult {

            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(0)]
        pub fn read_default_value(origin: OriginFor<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            let value = NumberStorage::<T>::get();

            //log::info!("Currnet stored value {}", value);
            Self::deposit_event(Event::<T>::DefaultValue {
                default_value: value,
                who: caller
            });
            Ok(())
        }
    }
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
use crate::weights::WeightInfo;
