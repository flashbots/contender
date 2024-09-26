// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import {ERC20} from "lib/openzeppelin-contracts/contracts/token/ERC20/ERC20.sol";

contract Token is ERC20("TestToken", "TKN") {
    constructor(uint256 initialSupply) {
        _mint(msg.sender, initialSupply);
    }
}
