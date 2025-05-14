#![cfg_attr(not(feature = "std"), no_std)]

pub mod wasm_compatiable {
    use super::*;
    use sp_std::vec::Vec;

    use codec::{Decode, Encode, MaxEncodedLen};
    use frame_support::__private::RuntimeDebug;
    use scale_info::TypeInfo;


    #[derive(Decode,Encode,Clone, PartialEq, Eq, Debug,Default)]
    pub struct Payload {
        pub job_id : Vec<u8>,
        pub job_content: Vec<u8>,
        pub job_state: JobState,
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

