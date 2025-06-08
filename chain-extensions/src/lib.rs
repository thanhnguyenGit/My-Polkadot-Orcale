#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::fungibles::{
    approvals::Inspect as AllowanceInspect, metadata::Inspect as MetadataInspect, Inspect,
};
use frame_support::DefaultNoBound;
use pallet_contracts::chain_extension::{
    ChainExtension, Environment, Ext, InitState, RetVal, SysConfig,RegisteredChainExtension,BufIn,BufOut
};
use codec::{Decode,Encode,MaxEncodedLen};
use sp_runtime::traits::{Get, StaticLookup};
use sp_runtime::{BoundedVec, DispatchError};
use sp_std::marker::PhantomData;
use wasmstore_pallet::{
    ScriptStorage as WSScriptStorage,
    Config as WSConfig
};

type Weight<T> = <T as wasmstore_pallet::Config>::WeightInfo;


#[derive(DefaultNoBound)]
pub struct MyCustomExtension<T>(PhantomData<T>);

impl<T> ChainExtension<T> for MyCustomExtension<T> where
    T: pallet_contracts::Config + wasmstore_pallet::Config,
    <<T as SysConfig>::Lookup as StaticLookup>::Source: From<<T as SysConfig>::AccountId>,
{
    fn call<E: Ext<T=T>>(&mut self, mut env: Environment<E, InitState>) -> pallet_contracts::chain_extension::Result<RetVal> {
        let mut env = env.buf_in_buf_out();
        match env.func_id() {
            0 => {
                // Decode the input: a script key as Vec<u8>
                let key: BoundedVec<u8,T::MaxScriptKeyLen> = env.read_as()?;
                let bounded_key: BoundedVec<_, <T as WSConfig>::MaxScriptKeyLen> =
                    BoundedVec::try_from(key.clone()).map_err(|_| DispatchError::Other("KeyTooLong"))?;

                let maybe_script = WSScriptStorage::<T>::get(&bounded_key);

                match maybe_script {
                    Some(script) => {
                        let encoded = script.encode();
                        env.write(&encoded, false, None)?;
                        Ok(RetVal::Converging(0))
                    }
                    None => Err(DispatchError::Other("ScriptNotFound")),
                }
            }
            _ => Err(DispatchError::Other("Unimplemented")),
        }
    }
}

#[macro_export]
macro_rules! handle_result {
    ($call_result:expr) => {{
        return match $call_result {
            Err(e) => {
                log::trace!(target: LOG_TARGET, "err: {:?}", e);
                let mapped_error = Outcome::from(e);
                Ok(RetVal::Converging(mapped_error as u32))
            }
            Ok(_) => Ok(RetVal::Converging(Outcome::Success as u32)),
        };
    }};
}

#[macro_export]
macro_rules! selector_bytes {
    ($s:expr) => {{
        let hash = blake2_256($s.as_bytes());
        [hash[0], hash[1], hash[2], hash[3]]
    }};
}

