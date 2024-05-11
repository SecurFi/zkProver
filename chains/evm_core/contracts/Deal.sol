// SPDX-License-Identifier: MIT
pragma solidity >=0.6.2 <0.9.0;

import "./forge-std/Test.sol";

contract Deal is Test {

    function batchDeal(address[] calldata accounts, address[] calldata tokens, uint256[] calldata balances) external {
        require(tokens.length == balances.length, "tokens and balances must have the same length");
        for (uint256 i = 0; i < tokens.length; i++) {
            if (tokens[i] != address(0x0)) {
                deal(tokens[i], accounts[0], balances[i]);
            } else {
                deal(accounts[i], balances[i]);
            }
        }
    }
}