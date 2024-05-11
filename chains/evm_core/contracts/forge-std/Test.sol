// SPDX-License-Identifier: MIT
pragma solidity >=0.6.2 <0.9.0;

pragma experimental ABIEncoderV2;

// 🧩 MODULES
import {console2} from "./console2.sol";
import {StdCheats} from "./StdCheats.sol";
import {StdStorage, stdStorage} from "./StdStorage.sol";
import {Vm} from "./Vm.sol";

import {TestBase} from "./Base.sol";

// ⭐️ TEST
abstract contract Test is StdCheats, TestBase{
// Note: IS_TEST() must return true.
// Note: Must have failure system, https://github.com/dapphub/ds-test/blob/cd98eff28324bfac652e63a239a60632a761790b/src/test.sol#L39-L76.
}