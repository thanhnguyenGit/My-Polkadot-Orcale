#![cfg_attr(not(feature = "std"), no_std)]

pub mod wasm_compatiable {
    use super::*;
    use sp_std::vec::Vec;

    use codec::{Decode, Encode, MaxEncodedLen};
    use frame_support::__private::RuntimeDebug;
    use scale_info::TypeInfo;
    use sp_core::crypto::AccountId32;
    use sp_core::sr25519::{Public, Signature};

    #[derive(Decode,Encode,Clone, PartialEq, Eq, Debug,Default)]
    pub struct RequestPayload {
        pub job_id : Vec<u8>,
        pub job_content: Vec<u8>,
        pub content_abi : Vec<u8>,
        pub job_state: JobState,
    }
    #[derive(Decode,Encode,Clone, PartialEq, Eq, Debug,Default)]
    pub struct ResponePayload {
        pub job_id : Vec<u8>,
        pub job_result: Vec<u8>,
        pub job_state: JobState,
        pub address: Public,
        pub exe_time: u32,
    }

    #[derive(Encode,MaxEncodedLen,Default, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub enum JobState {
        #[default]
        Idling,
        Pending,
        Processing,
        Finish,
    }

}


