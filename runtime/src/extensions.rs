use super::Runtime;
// Chain extensions
pub use chain_extensions::MyCustomExtension;
use pallet_contracts::chain_extension::{ChainExtension, Environment, Ext, InitState, RegisteredChainExtension, RetVal};

impl RegisteredChainExtension<Runtime> for MyCustomExtension<Runtime> {
    const ID: u16 = 02;
}

pub type OracleChainExtensions<Runtime> = MyCustomExtension<Runtime>;