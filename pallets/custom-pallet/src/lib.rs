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
    use codec::{decode_from_bytes, decode_vec_with_len, Codec, KeyedVec};
    use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*, DefaultNoBound};
    use frame_support::traits::{
        dynamic_params::IntoKey,
        Currency,
        LockableCurrency,
        ReservableCurrency
    };
    use frame_system::pallet_prelude::*;
    use sp_core::Public;
    use sp_io::offchain::timestamp;
    use sp_runtime::traits::{CheckedAdd, One, Hash};
    use pallet_balances as PalletBalances;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_balances::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
        #[pallet::constant]
        type MinStakeAmount : Get<u32>;

    }

    #[pallet::storage]
    #[pallet::getter(fn get_staked_processor)]
    pub type StakedProcessors<T : Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Meta<T>, OptionQuery>;

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Meta<T: Config> {
        pub(crate) balance: T::Balance,
        pub(crate) credit_score: u32,
    }


    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ProcessorStaked {
            who: T::AccountId,
            amount: T::Balance
        },
    }
    #[pallet::error]
    pub enum Error<T> {
        InsufficientFund,
        AlreadyStaked,
    }
    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
        pub fn stake(origin: OriginFor<T>, stake_amount: T::Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // ensure!(!StakedProcessors::<T>::contains_key(&who), Error::<T>::AlreadyStaked);
            // ensure!(stake_amount.into() >= T::MinStakeAmount::get(), Error::<T>::InsufficientFund);
            match PalletBalances::Pallet::<T>::reserve(&who, stake_amount) {
                Ok(_) => {
                    log::info!("SUCCESS - msg: Reserve: {:?} succesfull for {:?}", stake_amount,who);
                    Self::deposit_event(Event::<T>::ProcessorStaked {
                        who,
                        amount: stake_amount
                    });
                }
                Err(e) => {
                    log::error!("{:?} - msg: Faild to reverse for {:?}",e,who)
                }
            }
            Ok(().into())
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
