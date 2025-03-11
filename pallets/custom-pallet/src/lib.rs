#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
use alloc::vec::Vec;
pub use pallet::*;
#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use super::*;
    use frame_support::dispatch::RawOrigin;
    use frame_support::pallet_prelude::*;
    use frame_system::{
        self as system,
        pallet_prelude::*,
        offchain::{
            CreateSignedTransaction, SendSignedTransaction
        }
    };
    use frame_support::sp_runtime::{
        generic::BlockId::Number,
        offchain
    };
    use frame_support::{
        dispatch::{DispatchResult,DispatchResultWithPostInfo},
        ensure,
        traits::{Get,EnsureOrigin},
    };
    use scale_info::prelude::vec::Vec;
    use frame_support::sp_runtime::offchain::{
        http,
        Duration,
    };
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

        /// OCWs submit an unsigned transaction which can be abuse by spam,
        /// provide a frequency via GracePeriod stop this
        /// send one every GRACE_PERIOD blocks
        // #[pallet::constant]
        // type GracePeriod : Get<BlockNumberFor<Self>>;
        /// Numbers of blocks cooldown after an unsigned transaction is included.
        /// Ensure only accept an unsigned transaction once every UnsignedInterval blocks
        // #[pallet::constant]
        // type UnsignedInterval: Get<BlockNumberFor<Self>>;
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

    #[pallet::storage]
    #[pallet::getter(fn get_url)]
    pub type UrlDataStorage<T: Config> = StorageMap<_,Twox64Concat,T::AccountId,Vec<Vec<u8>>,ValueQuery>;
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
        },
        InitializeDefaultValue {
            default_value: u32
        },
        ModifyValue {
            old_value: u32,
            new_value: u32,
            who: T::AccountId,
        },
        AddUrl {
            who: T::AccountId,
            url: Vec<u8>,
        },
        FetchedData {
            url: &'static str,
            data: &'static str,
        }
    }

    #[pallet::error]
    pub enum Error<T> {
        CounterValueExceedsMax,
        CounterValueBelowZero,
        CounterOverflow,
        UserInteractionOverflow,
        NotSudo,
        CannotBeZero,
        CannotNull,
    }

    type bytes = Vec<u8>;
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
            let mut default_value = 0;
            if NumberStorage::<T>::get() == 0 {
                NumberStorage::<T>::put(T::DefaultValue::get());
                default_value = T::DefaultValue::get();
            }
            Self::deposit_event(Event::<T>::InitializeDefaultValue {
                default_value
            });
            T::DbWeight::get().writes(1)
        }
        fn offchain_worker(block_number: BlockNumberFor<T>) {
            log::info!("Offchain workder trigger at block: {:?}",block_number);
            let parent_hash = <system::Pallet<T>>::block_hash(block_number - 1u32.into());
            log::debug!("Current block: {:?}, parent hash: {:?}", block_number,parent_hash);
            if let Err(e) = Self::ocw_do_fetch_data() {
                log::error!("Error fetching data: {}", e);
            }
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
        pub fn add_url(origin: OriginFor<T>,url: Vec<u8>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            ensure!(
                url.len() != 0,
                Error::<T>::CannotBeZero
            );

            UrlDataStorage::<T>::try_mutate(&caller,|value| -> Result<_, Error<T>> {
                value.push(url.clone());
                Ok(())
            })?;

            Self::deposit_event(Event::<T>::AddUrl {
                who: caller,
                url
            });

            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(0)]
        pub fn read_default_value(origin: OriginFor<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            let value = NumberStorage::<T>::get();

            Self::deposit_event(Event::<T>::DefaultValue {
                default_value: value,
                who: caller
            });
            Ok(())
        }

        #[pallet::call_index(7)]
        #[pallet::weight(0)]
        pub fn modify_value(origin: OriginFor<T>,value : u32) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            let old_value = NumberStorage::<T>::get();
            ensure!(
                value != 0,
                Error::<T>::CannotBeZero
            );
            NumberStorage::<T>::mutate(|val| {
                *val = value;
            });

            let new_value = NumberStorage::<T>::get();
            Self::deposit_event(Event::<T>::ModifyValue {
                old_value,
                new_value,
                who: caller
            });

            Ok(())
        }
    }
    impl<T: Config> Pallet<T> {
        pub fn ocw_do_fetch_data() -> Result<(), http::Error> {
            let duration = Duration::from_millis(2_000);
            let url = "https://catfact.ninja/fact";
            let request = http::Request::get(url);
            let pending = request
                .deadline(Default::default())
                .send()
                .map_err(|_| http::Error::IoError)?;
            let response = pending
                .try_wait(Default::default())
                .map_err(|_| http::Error::DeadlineReached)??;
            if response.code != 200u16 {
                log::warn!("Unexpected status code: {}", response.code);
                return Err(http::Error::Unknown)
            }
            let body = response
                .body()
                .collect::<bytes>();
            let body_str = alloc::str::from_utf8(&body)
                .map_err(|_| {
                    log::warn!("No UTF-8 body");
                    http::Error::Unknown})?;
            Self::deposit_event(Event::<T>::FetchedData {
                url,
                data: body_str
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
