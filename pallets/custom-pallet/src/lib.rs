#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
use alloc::vec::Vec;
use codec::{Decode, Encode};
use alloc::string::{
    String,
    ToString
};
use sp_core::offchain::Timestamp;

pub use pallet::*;

pub mod crypto {
    use super::KEY_TYPE;
    use sp_core::sr25519::Signature as Sr25519Signature;
    use sp_runtime::{
        app_crypto::{app_crypto, sr25519},
        traits::Verify,
        MultiSignature, MultiSigner
    };
    app_crypto!(sr25519, KEY_TYPE);

    pub struct TestAuthId;

    impl frame_system::offchain::AppCrypto<MultiSigner,MultiSignature> for TestAuthId {
        type RuntimeAppPublic = Public;
        type GenericPublic = sp_core::sr25519::Public;
        type GenericSignature = sp_core::sr25519::Signature;
    }
}
#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use super::*;
    use frame_support::dispatch::RawOrigin;
    use frame_support::pallet_prelude::*;
    use frame_system::{
        self as system,
        pallet_prelude::*,
        offchain::{
            CreateSignedTransaction, SendSignedTransaction,SubmitTransaction,AppCrypto,SendUnsignedTransaction,
            SignedPayload,Signer,SigningTypes
        }
    };
    use frame_support::sp_runtime::offchain::{
        http,
        storage::{
            MutateStorageError,StorageRetrievalError,StorageValueRef
        },
        Duration,
    };
    use lite_json::json::JsonValue;
    use frame_support::sp_runtime::{
        generic::BlockId::Number,
        offchain
    };
    use frame_support::sp_runtime::{
        traits::Zero,
        transaction_validity::{InvalidTransaction,TransactionValidity,ValidTransaction}
    };
    use frame_support::{
        dispatch::{DispatchResult,DispatchResultWithPostInfo},
        ensure,
        traits::{Get,EnsureOrigin},
    };
    use frame_support::traits::Len;
    use scale_info::prelude::vec::Vec;
    use sp_core::crypto::KeyTypeId;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub initial_value: u32,
        pub _marker: PhantomData<T>,
    }

    // Configuration trait for the pallet
    #[pallet::config]
    pub trait Config: frame_system::Config + CreateSignedTransaction<Call<Self>> {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The identifier type for an OCW - off-chain worker
        type AuthorityId: AppCrypto<Self::Public, Self::Signature>;

        /// OCWs submit an unsigned transaction which can be abused by spam,
        /// provide a "block period" via GracePeriod to stop this,
        /// send one every GRACE_PERIOD blocks
        #[pallet::constant]
        type GracePeriod : Get<BlockNumberFor<Self>>;
        /// Numbers of blocks after an unsigned transaction is included.
        /// Ensure only accept an unsigned transaction once every UnsignedInterval blocks
        #[pallet::constant]
        type UnsignedInterval: Get<BlockNumberFor<Self>>;
        /// config for priority of unsigned transaction
        /// due to it dont't cost fee compare to signed.
        #[pallet::constant]
        type UnsignedPriority: Get<TransactionPriority>;
        #[pallet::constant]
        type CounterMaxValue: Get<u32>;
        #[pallet::constant]
        type DefaultValue: Get<u32>;
        /// A type representing the weights required by the dispatchables of this pallet
        #[pallet::constant]
        type MaxCatFact : Get<u32>;
        type WeightInfo: WeightInfo;
    }

    /// Storage for the current value of the counter.
    #[pallet::storage]
    pub(super) type CounterValue<T> = StorageValue<_, u32>;

    #[pallet::storage]
    #[pallet::getter(fn initial_value)]
    pub(super) type NumberStorage<T: Config> = StorageValue<_,u32,ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_url)]
    pub(super) type UrlDataStorage<T: Config> = StorageMap<_,Twox64Concat,T::AccountId,Vec<Vec<u8>>,ValueQuery>;
    /// Storage map to track the number of interactions performed by each account.
    #[pallet::storage]
    pub(super) type UserInteractions<T: Config> = StorageMap<_, Twox64Concat, T::AccountId, u32>;
    /// Define the block where next unsigned transaction will be accepted.
    /// Only one transaction every T::UnsignedInterval blocks.
    /// This value define when new transaction is going to be accepted
    #[pallet::storage]
    pub(super) type NextUnsignedAt<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_cat_fact)]
    pub(super) type CatFactStorage<T: Config> = StorageValue<_, BoundedVec<CatFact,T::MaxCatFact>,ValueQuery>;

    #[derive(Encode,Decode,Clone,PartialEq,Eq,RuntimeDebug,scale_info::TypeInfo)]
    pub struct CatFact {
        fact: String,
        length: u32
    }

    pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"cat!");

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
        AddData {
            who: Option<T::AccountId>,
            data: CatFact,
        },
        FetchedData {
            url: String,
            data: String,
        },
        UnsignedTxCommit {
            who: T::AccountId
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
        /// Off chain worker entry point.
        /// This function will be called when the node is fully synced and a new
        /// block is successfully imported.
        /// Note that it's not guaranteed for offchain workers to run on EVERY block, there might
        /// be cases where some blocks are skipped, or for some the worker runs twice (re-orgs),
        /// so the code should be able to handle that.
        /// You can use `Local Storage` API to coordinate runs of the worker.
        fn offchain_worker(block_number: BlockNumberFor<T>) {
            log::info!("Offchain workder trigger at block: {:?}",block_number);
            let parent_hash = <system::Pallet<T>>::block_hash(block_number - 1u32.into());
            log::info!("Current block: {:?}, parent hash: {:?}", block_number,parent_hash);
            if let Err(e) = Self::ocw_fetch_data_and_send_unsigned_tx(block_number) {
                log::error!("Error fetching data: {:#?}", e);
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

        #[pallet::call_index(8)]
        #[pallet::weight(0)]
        pub fn submit_unsigned_tx(origin: OriginFor<T>,_block_number: BlockNumberFor<T>, catfact: CatFact) -> DispatchResultWithPostInfo {
            let caller = ensure_none(origin)?;
            Self::add_cat_fact(None,catfact);
            let current_block = <system::Pallet<T>>::block_number();
            <NextUnsignedAt<T>>::put(current_block + T::UnsignedInterval::get());
            Self::deposit_event(Event::<T>::UnsignedTxCommit {
                who: caller
            });
            Ok(().into())
        }
    }
    impl<T: Config> Pallet<T> {
        /// fetch data and send an unsigned transaction.
        fn ocw_fetch_data_and_send_unsigned_tx(block_number: BlockNumberFor<T>) -> Result<(), &'static str> {
            let next_unsigned_at = NextUnsignedAt::<T>::get();
            if next_unsigned_at > block_number {
                return Err("InvalidOperator: need to wait before send another unsigned transaction");
            }
            let catfact = Self::ocw_do_fetch_data().map_err(|_| "Failed to fetch data")?;
            let call = Call::submit_unsigned_tx {block_number, catfact};
            
            Ok(())
        }
        fn ocw_do_fetch_data() -> Result<CatFact, http::Error> {
            let ttl: Timestamp = Timestamp::from_unix_millis(2000);
            let url = "https://catfact.ninja/fact";
            let request = http::Request::get(url);
            let pending = request
                .deadline(ttl)
                .send()
                .map_err(|_| http::Error::IoError)?;
            let response = pending
                .try_wait(ttl)
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
            let result = match Self::parse_responsee_to_json(body_str) {
                None => {
                    log::warn!("Unable to extract cat fact from the response: {:?}", body_str);
                    Err(http::Error::Unknown)
                }
                Some(cat_fact) => {
                    log::debug!("Cat fact: {:#?}", cat_fact);
                    Ok(cat_fact)
                }
            }.expect("Parser not working");
            Ok(result)
        }
        fn parse_responsee_to_json(response: &str) -> Option<CatFact>{
            let val = lite_json::parse_json(response).ok()?;

            if let JsonValue::Object(obj) = val {
                let fact = obj
                    .iter()
                    .find(|(k, _)| k.iter().copied().eq("fact".chars()))
                    .and_then(|(_, v)| match v {
                        JsonValue::String(fact) => Some(fact.iter().collect::<String>()),
                        _ => None,
                    })?;

                let length = obj
                    .iter()
                    .find(|(k, _)| k.iter().copied().eq("length".chars()))
                    .and_then(|(_, v)| match v {
                        JsonValue::Number(number) => Some(number.integer as u32),
                        _ => None,
                    })?;

                Some(CatFact {
                    fact,
                    length
                })
            } else {
                None
            }
        }
        fn add_cat_fact(caller: Option<T::AccountId>, new_cat_fact: CatFact) {
            let log_event_new_cat_fact = new_cat_fact.clone();
            log::info!("Adding cat fact to the on-chain list: {:#?}", new_cat_fact);
            CatFactStorage::<T>::mutate(|cat_facts| {
                if cat_facts.len() as u32 >= T::MaxCatFact::get()  {
                    log::info!("Number of Cat fact reach maximum, auto-remove old cat fact ");
                    cat_facts.remove(0);
                }
                let _ = cat_facts.try_push(new_cat_fact).map_err(|_| "StorageOverflow");
            });
            Self::deposit_event(Event::<T>::AddData {
                who: caller,
                data: log_event_new_cat_fact
            })
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
