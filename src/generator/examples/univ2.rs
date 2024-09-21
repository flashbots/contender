use crate::generator::{Generator, NamedTxRequest};
use crate::Result;
use alloy::primitives::{Address, TxKind, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy::sol;
use alloy::sol_types::SolCall;
use lazy_static::lazy_static;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct UniV2Spammer;

lazy_static! {
    static ref UNIV2_ROUTER02: Address = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
        .parse::<Address>()
        .unwrap();
}

sol! {
    #[sol(rpc)]
    contract UniswapV2Router02 {
        #[derive(Debug)]
        function addLiquidity(
            address tokenA,
            address tokenB,
            uint amountADesired,
            uint amountBDesired,
            uint amountAMin,
            uint amountBMin,
            address to,
            uint deadline
        ) external virtual override
        ensure(deadline) // ?
        returns (uint amountA, uint amountB, uint liquidity);
    }
}

/// Create a transaction to add liquidity to a Uniswap V2 pool.
fn tx_add_liquidity(
    token_a: Address,
    token_b: Address,
    amount_a: U256,
    amount_b: U256,
    min_amount_a: U256,
    min_amount_b: U256,
    to: Address,
    deadline: U256,
) -> Result<TransactionRequest> {
    let data = UniswapV2Router02::addLiquidityCall::new((
        token_a,
        token_b,
        amount_a,
        amount_b,
        min_amount_a,
        min_amount_b,
        to,
        deadline,
    ))
    .abi_encode();

    Ok(TransactionRequest {
        to: Some(TxKind::Call(UNIV2_ROUTER02.to_owned())),
        input: TransactionInput::both(data.into()),
        ..Default::default()
    })
}

impl Generator for UniV2Spammer {
    fn get_txs(
        &self,
        // TODO: implement these params
        _amount: usize,
    ) -> Result<Vec<NamedTxRequest>> {
        let token_a = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
            .parse::<Address>()
            .unwrap();
        let token_b = "0x6B175474E89094C44Da98b954EedeAC495271d0F"
            .parse::<Address>()
            .unwrap();
        let amount_a = U256::from(1_000_000);
        let amount_b = U256::from(1_000_000);
        let min_amount_a = U256::from(1_000);
        let min_amount_b = U256::from(1_000);
        let to = Address::repeat_byte(0x01);
        let deadline = U256::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get system time")
                .as_secs(),
        );

        // TODO: loop to generate multiple transactions with varying amounts

        let tx_req = tx_add_liquidity(
            token_a,
            token_b,
            amount_a,
            amount_b,
            min_amount_a,
            min_amount_b,
            to,
            deadline,
        )?;
        Ok(vec![tx_req.into()])
    }
}

#[cfg(test)]
mod tests {}
