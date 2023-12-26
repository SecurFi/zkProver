pub use alloy_primitives::{Address, address, BlockHash, BlockNumber, Bloom, Bytes, B256, B64, U256};
use ethers_core::types::{H160, H256, U256 as EthersU256, Bytes as EthersBytes};


pub trait ToAlloy {
    type To;
    fn to_alloy(self) -> Self::To;
}

impl ToAlloy for H256 {
    type To = B256;
    #[inline(always)]
    fn to_alloy(self) -> Self::To {
        B256::new(self.0)
    }
}

impl ToAlloy for EthersU256 {
    type To = U256;

    #[inline(always)]
    fn to_alloy(self) -> Self::To {
        U256::from_limbs(self.0)
    }
}

impl ToAlloy for H160 {
    type To = Address;
    #[inline(always)]
    fn to_alloy(self) -> Self::To {
        Address::new(self.0)
    }
}

impl ToAlloy for EthersBytes {
    type To = Bytes;
    #[inline(always)]
    fn to_alloy(self) -> Self::To {
        Bytes(self.0)
    }
}

pub trait ToEthers {
    type To;
    fn to_ethers(self) -> Self::To;
}

impl ToEthers for Address {
    type To = H160;
    #[inline(always)]
    fn to_ethers(self) -> Self::To {
        H160(self.0 .0)
    }
}

impl ToEthers for U256 {
    type To = EthersU256;
    #[inline(always)]
    fn to_ethers(self) -> Self::To {
        EthersU256(self.into_limbs())
    }
}