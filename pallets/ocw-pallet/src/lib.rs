#![cfg_attr(not(feature = "std"), no_std)]

use alloc::rc::Rc;
use codec::{Decode, Encode};
use frame_support::traits::Get;
use frame_system::{
    self as system,
    offchain::{
        AppCrypto, CreateSignedTransaction, SendSignedTransaction, SendUnsignedTransaction,
        SignedPayload, Signer, SigningTypes, SubmitTransaction,
    },
};
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;
use lite_json::json::JsonValue;
use sp_core::crypto::KeyTypeId;
use sp_runtime::{offchain::{
    http,
    storage::{MutateStorageError, StorageRetrievalError, StorageValueRef},
    Duration,
}, traits::Zero, transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction}, BoundedVec, RuntimeDebug};
use sp_std::vec::Vec;
extern crate alloc;
use alloc::string::{String,ToString};
use sp_core::ConstU32;
use sp_core::hexdisplay::AsBytesRef;
use sp_core::offchain::StorageKind;
use sp_io::offchain::{timestamp,local_storage_clear,local_storage_get,local_storage_set,local_storage_compare_and_set};
use sp_runtime::offchain::storage_lock::{BlockAndTime, StorageLock};
use sp_runtime::traits::{BlockNumberProvider, Hash};
use sp_runtime::format;
/// Defines application identifier for crypto keys of this module.
///
/// Every module that deals with signatures needs to declare its unique identifier for
/// its crypto keys.
/// When offchain worker is signing transactions it's going to request keys of type
/// `KeyTypeId` from the keystore and use the ones it finds to sign the transaction.
/// The keys can be inserted manually via RPC (see `author_insertKey`).
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"cat!");

/// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrappers.
/// We can use from supported crypto kinds (`sr25519`, `ed25519` and `ecdsa`) and augment
/// the types with this pallet-specific identifier.
pub mod crypto {
    use super::KEY_TYPE;
    use sp_core::sr25519::Signature as Sr25519Signature;
    use sp_runtime::{
        app_crypto::{app_crypto, sr25519},
        traits::Verify,
        MultiSignature, MultiSigner,
    };
    app_crypto!(sr25519, KEY_TYPE);

    pub struct TestAuthId;

    impl frame_system::offchain::AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
        type RuntimeAppPublic = Public;
        type GenericPublic = sp_core::sr25519::Public;
        type GenericSignature = sp_core::sr25519::Signature;
    }

    // implemented for mock runtime in test
    impl frame_system::offchain::AppCrypto<<Sr25519Signature as Verify>::Signer, Sr25519Signature>
    for TestAuthId
    {
        type RuntimeAppPublic = Public;
        type GenericPublic = sp_core::sr25519::Public;
        type GenericSignature = sp_core::sr25519::Signature;
    }
}

/// everything inside mode pallet is running in no-std.
#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_system::pallet_prelude::*;
    use frame_support::pallet_prelude::*;
    use sp_runtime::traits::{ensure_pow, BlockNumber};
    #[pallet::pallet]
    // #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: CreateSignedTransaction<Call<Self>> + frame_system::Config {
        /*
            The identifier type for an offchain worker.
            Use for when using OCW to make a sigened transaction
         */
        type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        // Configuration parameters
        #[pallet::constant]
        type GracePeriod: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type UnsignedInterval: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type UnsignedPriority: Get<TransactionPriority>;

        #[pallet::constant]
        type MaxPrices: Get<u32>;
    }


    /// A vector of recently submitted prices.
    ///
    /// This is used to calculate average price, should have bounded size.
    #[pallet::storage]
    #[pallet::getter(fn get_prices)]
    pub(super) type Prices<T: Config> = StorageValue<_, BoundedVec<u32, T::MaxPrices>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_api_url)]
    pub(super) type ApiURLStore<T: Config> = StorageDoubleMap<_,Blake2_128Concat,T::AccountId, Blake2_128Concat,u32, BoundedVec<u8,ConstU32<256>>,ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_hash_value)]
    pub(super) type HashStore<T: Config> = StorageMap<_,Blake2_128Concat,BoundedVec<u8, ConstU32<256>>,T::Hash,OptionQuery>;

    /// Defines the block when next unsigned transaction will be accepted.
    ///
    /// To prevent spam of unsigned (and unpayed!) transactions on the network,
    /// we only allow one transaction every `T::UnsignedInterval` blocks.
    /// This storage entry defines when new transaction is going to be accepted.
    #[pallet::storage]
    #[pallet::getter(fn next_unsigned_at)]
    pub(super) type NextUnsignedAt<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

    /// Events for the pallet.
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Event generated when new price is accepted to contribute to the average.
        NewPrice { price: u32, maybe_who: Option<T::AccountId> },
        UnsignedTx {
            when: BlockNumberFor<T>
        },
        AddNewUrl {
            who: T::AccountId,
            size: u32
        }
    }

    #[pallet::error]
    pub enum Error<T> {
        UnresolveOrigin,
        UrlTooLong,
        BytesTooLong,
        HashingProblem
    }
    pub const OCW_STORAGE_KEY : &[u8] = b"ocw-worker";
    pub const LOCK_BLOCK_EXPIRATION: u32 = 3;
    pub const LOCK_TIMEOUT_EXPIRATION: u64 = 5000;
    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn offchain_worker(block_number: BlockNumberFor<T>) {
            log::info!("Hello World from offchain workers!");
            let parent_hash = <system::Pallet<T>>::block_hash(block_number - 1u32.into());
            log::debug!("Current block: {:?} (parent hash: {:?})", block_number, parent_hash);
            let average: Option<u32> = Self::average_price();
            log::debug!("Current price: {:?}", average);

            // For this example we are going to send both signed and unsigned transactions
            // depending on the block number.
            // Usually it's enough to choose one or the other.
            // let should_send = Self::choose_transaction_type(block_number);
            // let res = match should_send {
            //     TransactionType::Signed => Self::fetch_price_and_send_signed(),
            //     TransactionType::UnsignedForAny =>
            //         Self::fetch_price_and_send_unsigned_for_any_account(block_number),
            //     TransactionType::UnsignedForAll =>
            //         Self::fetch_price_and_send_unsigned_for_all_accounts(block_number),
            //     TransactionType::Raw => Self::fetch_price_and_send_raw_unsigned(block_number),
            //     TransactionType::None => Ok(()),
            // };

            let res = Self::ocw_do_fetch_price_and_send_raw_unsigned(block_number);
            if let Err(e) = res {
                log::error!("Error: {}", e);
            }
        }
    }

    /// A public part of the pallet.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(0)]
        pub fn submit_price(origin: OriginFor<T>, price: u32) -> DispatchResultWithPostInfo {
            // Retrieve sender of the transaction.
            let who = ensure_signed(origin)?;
            // Add the price to the on-chain list.
            Self::add_data(Some(who), price);
            Ok(().into())
        }
        #[pallet::weight(0)]
        pub fn submit_price_unsigned(
            origin: OriginFor<T>,
            _block_number: BlockNumberFor<T>,
            price: u32,
        ) -> DispatchResultWithPostInfo {
            // This ensures that the function can only be called via unsigned transaction.
            ensure_none(origin)?;
            // Add the price to the on-chain list, but mark it as coming from an empty address.
            Self::add_data(None, price);
            // now increment the block number at which we expect next unsigned transaction.
            let current_block = <system::Pallet<T>>::block_number();
            <NextUnsignedAt<T>>::put(current_block + T::UnsignedInterval::get());
            Ok(().into())
        }

        #[pallet::weight(0)]
        pub fn submit_price_unsigned_with_signed_payload(
            origin: OriginFor<T>,
            price_payload: PricePayload<T::Public, BlockNumberFor<T>>,
            _signature: T::Signature,
        ) -> DispatchResultWithPostInfo {
            // This ensures that the function can only be called via unsigned transaction.
            ensure_none(origin)?;
            // Add the price to the on-chain list, but mark it as coming from an empty address.
            Self::add_data(None, price_payload.price);
            // now increment the block number at which we expect next unsigned transaction.
            let current_block = <system::Pallet<T>>::block_number();
            <NextUnsignedAt<T>>::put(current_block + T::UnsignedInterval::get());
            Ok(().into())
        }
        #[pallet::weight(0)]
        pub fn submit_api_url(origin : OriginFor<T>, topic_id: u32,api_url: String) -> DispatchResultWithPostInfo {
            let _singer = match ensure_signed_or_root(origin)? {
                Some(origin) => {
                    let api_url_bytes: BoundedVec<u8, ConstU32<256>> = api_url
                        .as_bytes()
                        .to_vec()
                        .try_into()
                        .map_err(|_| {
                            log::error!("Url too long");
                            Error::<T>::UrlTooLong
                        }).expect("Good");
                    let url_size = api_url_bytes.len() as u32;
                    ApiURLStore::<T>::insert(origin.clone(), topic_id, api_url_bytes);
                    Self::deposit_event(Event::<T>::AddNewUrl {
                        who: origin,
                        size: url_size
                    });
                },
                None => {
                    log::error!("Unresolve origin, extrensic called failed");
                    Error::<T>::UnresolveOrigin;
                    return Err(DispatchError::BadOrigin.into());
                }
            };
            Ok(().into())
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        /// Validate unsigned call to this module.
        ///
        /// By default unsigned transactions are disallowed, but implementing the validator
        /// here we make sure that some particular calls (the ones produced by offchain worker)
        /// are being whitelisted and marked as valid.
        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            // Firstly let's check that we call the right function.
            if let Call::submit_price_unsigned_with_signed_payload {
                price_payload: ref payload,
                ref signature,
            } = call
            {
                let signature_valid =
                    SignedPayload::<T>::verify::<T::AuthorityId>(payload, signature.clone());
                if !signature_valid {
                    return InvalidTransaction::BadProof.into()
                }
                Self::validate_transaction_parameters(&payload.block_number, &payload.price)
            } else if let Call::submit_price_unsigned { block_number, price: new_price } = call {
                Self::validate_transaction_parameters(block_number, new_price)
            } else {
                InvalidTransaction::Call.into()
            }
        }
    }

}

/// Payload used by this example crate to hold price
/// data required to submit a transaction.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, scale_info::TypeInfo)]
pub struct PricePayload<Public, BlockNumber> {
    block_number: BlockNumber,
    price: u32,
    public: Public,
}

impl<T: SigningTypes> SignedPayload<T> for PricePayload<T::Public, BlockNumberFor<T>> {
    fn public(&self) -> T::Public {
        self.public.clone()
    }
}

enum TransactionType {
    Signed,
    UnsignedForAny,
    UnsignedForAll,
    Raw,
    None,
}
enum StorageType {
    Persistent,
    Local
}

impl<T: Config> Pallet<T> {
    /// Chooses which transaction type to send.
    ///
    /// This function serves mostly to showcase `StorageValue` helper
    /// and local storage usage.
    ///
    /// Returns a type of transaction that should be produced in current run.
    fn ocw_do_choose_transaction_type(block_number: BlockNumberFor<T>) -> TransactionType {
        /// A friendlier name for the error that is going to be returned in case we are in the grace
        /// period.
        const RECENTLY_SENT: () = ();

        // Start off by creating a reference to Local Storage value.
        // Since the local storage is common for all offchain workers, it's a good practice
        // to prepend your entry with the module name.
        let val = StorageValueRef::persistent(b"ocw-pallet::last_send");
        // The Local Storage is persisted and shared between runs of the offchain workers,
        // and offchain workers may run concurrently. We can use the `mutate` function, to
        // write a storage entry in an atomic fashion. Under the hood it uses `compare_and_set`
        // low-level method of local storage API, which means that only one worker
        // will be able to "acquire a lock" and send a transaction if multiple workers
        // happen to be executed concurrently.
        let res = val.mutate(|last_send: Result<Option<BlockNumberFor<T>>, StorageRetrievalError>| {
            match last_send {
                // If we already have a value in storage and the block number is recent enough
                // we avoid sending another transaction at this time.
                Ok(Some(block)) if block_number < block + T::GracePeriod::get() =>
                    Err(RECENTLY_SENT),
                // In every other case we attempt to acquire the lock and send a transaction.
                _ => Ok(block_number),
            }
        });

        // The result of `mutate` call will give us a nested `Result` type.
        // The first one matches the return of the closure passed to `mutate`, i.e.
        // if we return `Err` from the closure, we get an `Err` here.
        // In case we return `Ok`, here we will have another (inner) `Result` that indicates
        // if the value has been set to the storage correctly - i.e. if it wasn't
        // written to in the meantime.
        match res {
            // The value has been set correctly, which means we can safely send a transaction now.
            Ok(block_number) => {
                // Depending if the block is even or odd we will send a `Signed` or `Unsigned`
                // transaction.
                // Note that this logic doesn't really guarantee that the transactions will be sent
                // in an alternating fashion (i.e. fairly distributed). Depending on the execution
                // order and lock acquisition, we may end up for instance sending two `Signed`
                // transactions in a row. If a strict order is desired, it's better to use
                // the storage entry for that. (for instance store both block number and a flag
                // indicating the type of next transaction to send).
                let transaction_type = block_number % 3u32.into();
                if transaction_type == Zero::zero() {
                    TransactionType::Signed
                } else if transaction_type == BlockNumberFor::<T>::from(1u32) {
                    TransactionType::UnsignedForAny
                } else if transaction_type == BlockNumberFor::<T>::from(2u32) {
                    TransactionType::UnsignedForAll
                } else {
                    TransactionType::Raw
                }
            },
            // We are in the grace period, we should not send a transaction this time.
            Err(MutateStorageError::ValueFunctionFailed(RECENTLY_SENT)) => TransactionType::None,
            // We wanted to send a transaction, but failed to write the block number (acquire a
            // lock). This indicates that another offchain worker that was running concurrently
            // most likely executed the same logic and succeeded at writing to storage.
            // Thus we don't really want to send the transaction, knowing that the other run
            // already did.
            Err(MutateStorageError::ConcurrentModification(_)) => TransactionType::None,
        }
    }

    /// A helper function to fetch the price and send signed transaction.
    fn ocw_do_fetch_price_and_send_signed() -> Result<(), &'static str> {
        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            return Err(
                "No local accounts available. Consider adding one via `author_insertKey` RPC.",
            )
        }
        // Make an external HTTP request to fetch the current price.
        // Note this call will block until response is received.
        let price = Self::fetch_data("https://catfact.ninja/fact").map_err(|_| "Failed to fetch price")?;
        // Using `send_signed_transaction` associated type we create and submit a transaction
        // representing the call, we've just created.
        // Submit signed will return a vector of results for all accounts that were found in the
        // local keystore with expected `KEY_TYPE`.
        let results = signer.send_signed_transaction(|_account| {
            // Received price is wrapped into a call to `submit_price` public function of this
            // pallet. This means that the transaction, when executed, will simply call that
            // function passing `price` as an argument.
            Call::submit_price { price }
        });

        for (acc, res) in &results {
            match res {
                Ok(()) => log::info!("[{:?}] Submitted price of {} cents", acc.id, price),
                Err(e) => log::error!("[{:?}] Failed to submit transaction: {:?}", acc.id, e),
            }
        }

        Ok(())
    }

    /// A helper function to fetch the price and send a raw unsigned transaction.
    fn ocw_do_fetch_price_and_send_raw_unsigned(block_number: BlockNumberFor<T>) -> Result<(), &'static str> {
        // Make sure we don't fetch the price if unsigned transaction is going to be rejected
        // anyway.
        let next_unsigned_at = <NextUnsignedAt<T>>::get();
        if next_unsigned_at > block_number {
            log::info!("Next unsigned at block: {:#?}, current block at: {:#?} ", next_unsigned_at,block_number);
            return Err("Too early to send unsigned transaction")
        }
        // Make an external HTTP request to fetch the current price.
        // Note this call will block until response is received.
        let price = Self::fetch_data("https://catfact.ninja/fact").map_err(|_| "Failed to fetch price")?;

        // Received price is wrapped into a call to `submit_price_unsigned` public function of this
        // pallet. This means that the transaction, when executed, will simply call that function
        // passing `price` as an argument.
        let call = Call::submit_price_unsigned { block_number, price };

        // Now let's create a transaction out of this call and submit it to the pool.
        // Here we showcase two ways to send an unsigned transaction / unsigned payload (raw)
        //
        // By default unsigned transactions are disallowed, so we need to whitelist this case
        // by writing `UnsignedValidator`. Note that it's EXTREMELY important to carefuly
        // implement unsigned validation logic, as any mistakes can lead to opening DoS or spam
        // attack vectors. See validation logic docs for more details.
        //
        log::info!("Data fetced: {}, preparing to sent transaction",price);
        SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
            .map_err(|()| "Unable to submit unsigned transaction.")?;
        Self::deposit_event(Event::<T>::UnsignedTx {
                when: block_number
        });
        Ok(())
    }

    /// A helper function to fetch the price, sign payload and send an unsigned transaction
    fn ocw_do_fetch_price_and_send_unsigned_for_any_account(
        block_number: BlockNumberFor<T>,
    ) -> Result<(), &'static str> {
        // Make sure we don't fetch the price if unsigned transaction is going to be rejected
        // anyway.
        let next_unsigned_at = <NextUnsignedAt<T>>::get();
        if next_unsigned_at > block_number {
            return Err("Too early to send unsigned transaction")
        }

        // Make an external HTTP request to fetch the current price.
        // Note this call will block until response is received.
        let price = Self::fetch_data("https://catfact.ninja/fact").map_err(|_| "Failed to fetch price")?;

        // -- Sign using any account
        let (_, result) = Signer::<T, T::AuthorityId>::any_account()
            .send_unsigned_transaction(
                |account| PricePayload { price, block_number, public: account.public.clone() },
                |payload, signature| Call::submit_price_unsigned_with_signed_payload {
                    price_payload: payload,
                    signature,
                },
            )
            .ok_or("No local accounts accounts available.")?;
        result.map_err(|()| "Unable to submit transaction")?;

        Ok(())
    }

    /// A helper function to fetch the price, sign payload and send an unsigned transaction
    fn ocw_do_fetch_price_and_send_unsigned_for_all_accounts(
        block_number: BlockNumberFor<T>,
    ) -> Result<(), &'static str> {
        // Make sure we don't fetch the price if unsigned transaction is going to be rejected
        // anyway.
        let next_unsigned_at = <NextUnsignedAt<T>>::get();
        if next_unsigned_at > block_number {
            return Err("Too early to send unsigned transaction")
        }
        // Make an external HTTP request to fetch the current price.
        // Note this call will block until response is received.
        let price = Self::fetch_data("https://catfact.ninja/fact").map_err(|_| "Failed to fetch price")?;

        // -- Sign using all accounts
        let transaction_results = Signer::<T, T::AuthorityId>::all_accounts()
            .send_unsigned_transaction(
                |account| PricePayload { price, block_number, public: account.public.clone() },
                |payload, signature| Call::submit_price_unsigned_with_signed_payload {
                    price_payload: payload,
                    signature,
                },
            );
        for (_account_id, result) in transaction_results.into_iter() {
            if result.is_err() {
                return Err("Unable to submit transaction")
            }
        }
        Ok(())
    }
    fn fetch_data_and_store_hash(block_number: BlockNumberFor<T>) -> Result<(), &'static str> {
        let next_unsigned_at = <NextUnsignedAt<T>>::get();
        if next_unsigned_at > block_number {
            return Err("Too early to send unsigned transaction")
        }

        let price = Self::fetch_data("https://catfact.ninja/fact").map_err(|_| "Failed to fetch price")?;
        Ok(())
    }

    /// Fetch current price and return the result in cents.
    fn fetch_data(url : &str) -> Result<u32, http::Error> {
        let body = Self::response_from_http_url(url,5_000)?;
        // log::info!("RESPONSE BODY RAW: {:#?}", body);
        let body_str = sp_std::str::from_utf8(&body).map_err(|_| {
            log::warn!("No UTF8 body");
            http::Error::Unknown
        })?;
        log::info!("RESPONSE BODY: {:#?}", body_str.to_string());
        let time_stamp = timestamp().unix_millis();
        let block_height = <frame_system::Pallet<T>>::block_number();
        let key = format!("ocw-pallet:catfact:{:?}:{:?}:{:?}", time_stamp, block_height,T::Hashing::hash(&body));
        log::info!("KEY ADDED: {}", key);
        let key_as_bytes = Self::to_bounded_vec::<_,256>(key.clone()).expect("Data");
        log::info!("KEY INTO Bytes {:?}", &key_as_bytes);
        if !HashStore::<T>::contains_key(&key_as_bytes) {
            log::debug!("Hash store has no key {:?}", key);
            Self::build_local_storage_for_http_response(StorageKind::PERSISTENT, key.as_bytes().clone(), &body);
            HashStore::<T>::insert(&key_as_bytes, T::Hashing::hash(&body));
            log::info!("Succes storing data onchain and offchain.")
        }
        let price = match Self::parse_data(body_str) {
            Some(price) => Ok(price),
            None => {
                log::warn!("Unable to extract data from the response: {:?}", body_str);
                Err(http::Error::Unknown)
            },
        }?;

        log::info!("Got price: {} cents", price);

        Ok(price)
    }

    fn parse_data(price_str: &str) -> Option<u32> {
        let val = lite_json::parse_json(price_str);
        let price = match val.ok()? {
            JsonValue::Object(obj) => {
                let (_, v) = obj.into_iter().find(|(k, _)| k.iter().copied().eq("length".chars()))?;
                match v {
                    JsonValue::Number(number) => number,
                    _ => return None,
                }
            },
            _ => return None,
        };
        let exp = price.integer;
        log::info!("Parse res: {}", exp);
        Some(price.integer as u32)
    }

    /// Add new price to the list.
    fn add_data(maybe_who: Option<T::AccountId>, price: u32) {
        log::info!("Adding to the average: {}", price);
        <Prices<T>>::mutate(|prices| {
            if prices.try_push(price).is_err() {
                prices[(price % T::MaxPrices::get()) as usize] = price;
            }
        });

        let average = Self::average_price()
            .expect("The average is not empty, because it was just mutated; qed");
        log::info!("Current average price is: {}", average);
        // here we are raising the NewPrice event
        Self::deposit_event(Event::NewPrice { price, maybe_who });
    }

    /// Calculate current average price.
    fn average_price() -> Option<u32> {
        let prices = <Prices<T>>::get();
        if prices.is_empty() {
            None
        } else {
            Some(prices.iter().fold(0_u32, |a, b| a.saturating_add(*b)) / prices.len() as u32)
        }
    }

    fn validate_transaction_parameters(
        block_number: &BlockNumberFor<T>,
        new_price: &u32,
    ) -> TransactionValidity {
        let next_unsigned_at = <NextUnsignedAt<T>>::get();
        if &next_unsigned_at > block_number {
            log::warn!("Unsigned Transaction period still in cool down");
            log::warn!("block number: {:#?}, next unsigned at: {:#?}",block_number,next_unsigned_at);
            return InvalidTransaction::Stale.into()
        }
        let current_block = <system::Pallet<T>>::block_number();
        if &current_block < block_number {
            log::warn!("Unsigned Transaction period corrupted");
            log::warn!("block number: {:#?}, next unsigned at: {:#?}",block_number,next_unsigned_at);
            return InvalidTransaction::Future.into()
        }
        let avg_price = Self::average_price()
            .map(|price| if &price > new_price { price - new_price } else { new_price - price })
            .unwrap_or(0);

        ValidTransaction::with_tag_prefix("OffchainWorker")
            .priority(T::UnsignedPriority::get().saturating_add(avg_price as _))
            .and_provides(next_unsigned_at)
            .longevity(5)
            .propagate(true)
            .build()
    }
    fn response_from_http_url(url: &str, time_to_live: u64) -> Result< Vec<u8>, http::Error> {
        let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(time_to_live));
        let request =
            http::Request::get(url);
        let pending = request.deadline(deadline).send().map_err(|_| http::Error::IoError)?;
        let response = pending.try_wait(deadline).map_err(|_| http::Error::DeadlineReached)??;
        if response.code != 200 {
            log::warn!("Unexpected status code: {}", response.code);
            return Err(http::Error::Unknown)
        }
        Ok(response.body().collect::<Vec<u8>>())
    }
    fn build_local_storage_for_http_response(storage_type: StorageKind,key : &[u8],data: &[u8]) {
        match storage_type {
            StorageKind::PERSISTENT => {
                let storage = StorageValueRef::persistent(key);
                log::info!("Adding data to offchain storage");
                if let Ok(Some(value)) = storage.get::<BoundedVec<u8, ConstU32<512>>>() {
                    log::info!("Value already exist of the key {:?}: {:?}",OCW_STORAGE_KEY,value);
                }
                local_storage_set(storage_type, key, data);
            },
            StorageKind::LOCAL => {
                let storage = StorageValueRef::local(key);
                log::info!("Adding data to offchain storage");
                if let Ok(Some(value)) = storage.get::<BoundedVec<u8, ConstU32<512>>>() {
                    log::info!("Value already exist of the key {:?}: {:?}",OCW_STORAGE_KEY,value);
                }
                local_storage_set(storage_type, key, data);
            }
        }
    }
    fn to_bounded_vec<I, const N: u32>(input: I) -> Result<BoundedVec<u8, ConstU32<N>>, &'static str>
    where
        I: Into<Vec<u8>>,
    {
        let vec: Vec<u8> = input.into();
        BoundedVec::try_from(vec).map_err(|_| "Input data is too large")
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
