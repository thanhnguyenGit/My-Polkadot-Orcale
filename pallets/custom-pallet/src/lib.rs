#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use codec::{Decode, Encode};
use sp_core::offchain::Timestamp;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_support::sp_runtime::offchain::{http, storage::StorageValueRef, Duration};
    use frame_support::sp_runtime::{
        traits::Zero,
        transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
    };
    use frame_system::{
        self as system,
        offchain::{SendUnsignedTransaction, SubmitTransaction},
        pallet_prelude::*,
    };
    use lite_json::json::JsonValue;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::genesis_config]
    pub struct GenesisConfig<T> {
        pub _marker: PhantomData<T>,
    }

    #[pallet::config]
    pub trait Config:
        frame_system::Config + frame_system::offchain::SendTransactionTypes<Call<Self>>
    {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        #[pallet::constant]
        type GracePeriod: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type UnsignedInterval: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type UnsignedPriority: Get<TransactionPriority>;
        #[pallet::constant]
        type MaxCatFact: Get<u32>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::storage]
    pub(super) type NextUnsignedAt<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_cat_fact)]
    pub(super) type CatFactStorage<T: Config> =
        StorageValue<_, BoundedVec<CatFact, T::MaxCatFact>, ValueQuery>;

    #[pallet::storage]
    pub type InitValue<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[derive(
        Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, scale_info::TypeInfo,
    )]
    pub struct CatFact {
        fact: BoundedVec<u8, ConstU32<128>>,
        length: u32,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        NetworkRunning {
            msg: String,
        },
        AddData {
            who: Option<T::AccountId>,
            data: CatFact,
        },
        FetchedData {
            url: String,
            data: BoundedVec<u8, ConstU32<128>>,
        },
        UnsignedTxCommit {
            who: Option<T::AccountId>,
        },
        GenesisEvent {
            init_value: BoundedVec<CatFact,T::MaxCatFact>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        CannotBeZero,
        CannotNull,
    }
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                _marker: Default::default(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            log::debug!("Log debug work");
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_block_number: BlockNumberFor<T>) -> Weight {
            log::debug!("Log debug work on init");
            log::info!("Log info work");
            InitValue::<T>::put(12);
            T::DbWeight::get().writes(1)
        }
        fn offchain_worker(block_number: BlockNumberFor<T>) {
            log::info!("Offchain worker triggered at block: {:?}", block_number);
            let parent_hash = <system::Pallet<T>>::block_hash(block_number);
            log::info!(
                "Current block: {:?}, parent hash: {:?}",
                block_number,
                parent_hash
            );
            match Self::ocw_fetch_data_and_send_unsigned_tx(block_number) {
                Ok(()) => log::info!("OCW fetch and submit successful"),
                Err(e) => log::error!("OCW failed: {}", e),
            }
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;
        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            log::info!("Validating unsigned transaction");
            let valid_tx = |provide| ValidTransaction::with_tag_prefix("Oracle")
                .priority(T::UnsignedPriority::get())
                .and_provides([&provide])
                .longevity(3)
                .propagate(true)
                .build();
            match call {
                Call::submit_unsigned_tx {
                    block_number,
                    catfact,
                } => {
                    let current_block = <system::Pallet<T>>::block_number();
                    if *block_number > current_block {
                        return InvalidTransaction::Future.into();
                    }
                    if catfact.fact.is_empty() || catfact.length == 0 {
                        log::info!("Validating Fail, transaction is not valid");
                        return InvalidTransaction::Custom(1).into();
                    }
                    valid_tx(b"submit_unsigned_tx".to_vec())
                },
                _ => InvalidTransaction::Call.into(),
            }
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(0)]
        pub fn submit_unsigned_tx(
            origin: OriginFor<T>,
            _block_number: BlockNumberFor<T>,
            catfact: CatFact,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
            Self::add_cat_fact(None, catfact);
            let current_block = <system::Pallet<T>>::block_number();
            <NextUnsignedAt<T>>::put(current_block + T::UnsignedInterval::get());
            Self::deposit_event(Event::<T>::UnsignedTxCommit { who: None });
            Ok(().into())
        }
        #[pallet::call_index(1)]
        #[pallet::weight(0)]
        pub fn debig_emit_msg_event(origin: OriginFor<T>) -> DispatchResult {
            log::info!("INFO work");
            log::debug!("Debug work");
            let msg = "Network is running and extrensic working".to_string();
            Self::deposit_event(Event::<T>::NetworkRunning { msg });
            Ok(())
        }
        #[pallet::call_index(2)]
        #[pallet::weight(0)]
        pub fn get_init_value(orign: OriginFor<T>) -> DispatchResult {
            let init_value = CatFactStorage::<T>::get();
            Self::deposit_event(Event::<T>::GenesisEvent { init_value });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn ocw_fetch_data_and_send_unsigned_tx(
            block_number: BlockNumberFor<T>,
        ) -> Result<(), &'static str> {
            let next_unsigned_at = <NextUnsignedAt<T>>::get();
            if next_unsigned_at > block_number {
                log::info!("Skipping fetch: next unsigned at {:?}", next_unsigned_at);
                return Err("Too early to fetch");
            }

            let catfact = Self::ocw_do_fetch_data().map_err(|_| "Failed to fetch data")?;
            Self::deposit_event(Event::<T>::FetchedData {
                url: "https://catfact.ninja/fact".to_string(),
                data: catfact.fact.clone(),
            });

            let call = Call::submit_unsigned_tx {
                block_number,
                catfact,
            };
            SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
                .map_err(|_| "Unable to submit unsigned transaction")?;
            log::info!("Unsigned transaction submitted at block {:?}", block_number);
            Ok(())
        }

        fn ocw_do_fetch_data() -> Result<CatFact, http::Error> {
            let ttl = Timestamp::from_unix_millis(10000);
            let url = "https://catfact.ninja/fact";
            let request = http::Request::get(url);
            let pending = request
                .deadline(ttl)
                .send()
                .map_err(|_| http::Error::IoError)?;
            let response = pending
                .try_wait(ttl)
                .map_err(|_| http::Error::DeadlineReached)??;
            if response.code != 200 {
                log::error!("Unexpected status code: {}", response.code);
                return Err(http::Error::Unknown);
            }
            let body = response.body().collect::<Vec<u8>>();
            let body_str = alloc::str::from_utf8(&body).map_err(|_| {
                log::error!("No UTF-8 body");
                http::Error::Unknown
            })?;
            let cat_fact = Self::parse_response_to_json(body_str).ok_or(http::Error::Unknown)?;
            log::info!("Fetched cat fact: {:?}", cat_fact);
            Ok(cat_fact)
        }

        fn parse_response_to_json(response: &str) -> Option<CatFact> {
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
                let fact_bytes = fact.as_bytes().to_vec();
                let fact_vec = BoundedVec::<u8, ConstU32<128>>::try_from(fact_bytes)
                    .map_err(|invalid| {
                        log::error!("Cat fact {:#?} is too long: {} bytes", invalid, fact.len());
                        ()
                    })
                    .ok()?;
                Some(CatFact {
                    fact: fact_vec,
                    length,
                })
            } else {
                None
            }
        }

        fn add_cat_fact(caller: Option<T::AccountId>, new_cat_fact: CatFact) {
            log::info!("Adding cat fact: {:?}", new_cat_fact);
            CatFactStorage::<T>::mutate(|cat_facts| {
                if cat_facts.len() as u32 >= T::MaxCatFact::get() {
                    log::info!("Max cat facts reached, removing oldest");
                    cat_facts.remove(0);
                }
                cat_facts
                    .try_push(new_cat_fact.clone())
                    .expect("Storage overflow");
            });
            Self::deposit_event(Event::<T>::AddData {
                who: caller,
                data: new_cat_fact,
            });
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
