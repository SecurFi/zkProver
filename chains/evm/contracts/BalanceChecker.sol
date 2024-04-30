// SPDX-License-Identifier: MIT
pragma solidity >=0.6.2 <0.9.0;

// ERC20 contract interface
interface IERC20 {
  function balanceOf(address) external view returns (uint);
}

contract BalanceChecker {

  function tokenBalance(address account, address token) public view returns (uint256) {
    uint256 tokenCode;
    assembly { tokenCode := extcodesize(token) }
    if (tokenCode > 0) {
      (bool success,) = token.staticcall(
        abi.encodeWithSelector(IERC20(token).balanceOf.selector, account)
      );
      if (success) {
        return IERC20(token).balanceOf(account);
      }
      
    }

    return 0;
  }

  function balances(address[] calldata users, address[] calldata tokens) external view returns (uint[] memory) {
    uint[] memory addrBalances = new uint[](tokens.length * users.length);
    
    for(uint i = 0; i < users.length; i++) {
      for (uint j = 0; j < tokens.length; j++) {
        uint addrIdx = j + tokens.length * i;
        if (tokens[j] != address(0x0)) { 
          addrBalances[addrIdx] = tokenBalance(users[i], tokens[j]);
        } else {
          addrBalances[addrIdx] = users[i].balance; // ETH balance    
        }
      }  
    }
  
    return addrBalances;
  }

}