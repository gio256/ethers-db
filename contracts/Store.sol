// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.4.24;

contract Store {
    uint256 public count;

    constructor() {
        count = 1;
    }

    function inc() external {
        count++;
    }
}
