use alloy_sol_types::sol;

include!(concat!(env!("OUT_DIR"), "/helper_contract.rs"));

sol! {
    interface Helper {
        function batchDeal(address[] calldata accounts, address[] calldata tokens, uint256[] calldata balances) external;
        function balances(address[] calldata users, address[] calldata tokens) external view returns (uint256[] memory);
    }
}

