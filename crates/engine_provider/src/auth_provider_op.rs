use crate::{AdvanceChain, util::read_jwt_file};
use alloy::{
    eips::Encodable2718,
    hex as make_hex,
    hex::ToHexExt,
    primitives::{B256, BlockHash, Bytes, TxKind, U256, address},
};
use alloy_rpc_types_engine::{ForkchoiceUpdated, PayloadAttributes};
use op_alloy_consensus::{OpTypedTransaction, TxDeposit};
use op_rbuilder::{
    tester::{EngineApi, EngineApiBuilder},
    tx_signer::Signer,
};
use reth_optimism_node::OpPayloadAttributes;
use std::{path::PathBuf, time::Duration};

const FJORD_DATA: &[u8] = &make_hex!(
    "440a5e200000146b000f79c500000000000000040000000066d052e700000000013ad8a3000000000000000000000000000000000000000000000000000000003ef1278700000000000000000000000000000000000000000000000000000000000000012fdf87b89884a61e74b322bbcf60386f543bfae7827725efaaf0ab1de2294a590000000000000000000000006887246668a3b87f54deb3b94ba47a6f63f32985"
);

pub struct AuthProviderOp {
    inner: EngineApi,
}

impl AuthProviderOp {
    /// Create a new AuthProvider instance.
    /// This will create a new authenticated transport connected to `auth_rpc_url` using `jwt_secret`.
    pub async fn new(
        auth_rpc_url: &str,
        jwt_secret: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let engine_client = EngineApiBuilder::new()
            .with_jwt_secret(jwt_secret)
            .with_url(auth_rpc_url)
            .build()?;
        Ok(Self {
            inner: engine_client,
        })
    }

    /// Create a new AuthProvider instance from a JWT secret file.
    /// The JWT secret is hex encoded and will be decoded after reading the file.
    pub async fn from_jwt_file(
        auth_rpc_url: &str,
        jwt_secret_file: &PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // fetch jwt from file
        let jwt = read_jwt_file(jwt_secret_file)?;
        let jwt_hex = jwt.as_bytes().encode_hex();
        Self::new(auth_rpc_url, &jwt_hex).await
    }
}

fn get_block_info_tx() -> Result<Bytes, Box<dyn std::error::Error>> {
    let deposit_tx = TxDeposit {
        source_hash: B256::default(),
        from: address!("DeaDDEaDDeAdDeAdDEAdDEaddeAddEAdDEAd0001"),
        to: TxKind::Call(address!("4200000000000000000000000000000000000015")),
        mint: None,
        value: U256::default(),
        gas_limit: 210000,
        is_system_transaction: false,
        input: FJORD_DATA.into(),
    };

    // Create a temporary signer for the deposit
    let signer = Signer::random();
    let signed_tx = signer.sign_tx(OpTypedTransaction::Deposit(deposit_tx))?;
    Ok(signed_tx.encoded_2718().into())
}

async fn call_fcu_default(
    engine_client: &EngineApi,
    current_head: BlockHash,
    new_head: BlockHash,
    timestamp: Option<u64>,
    gas_limit: Option<u64>,
) -> Result<ForkchoiceUpdated, Box<dyn std::error::Error>> {
    let info_tx = get_block_info_tx()?;
    let fcu_response = engine_client
        .update_forkchoice(
            current_head,
            new_head,
            timestamp.map(|timestamp| OpPayloadAttributes {
                payload_attributes: PayloadAttributes {
                    timestamp,
                    prev_randao: B256::ZERO,
                    suggested_fee_recipient: Default::default(),
                    withdrawals: Some(vec![]),
                    parent_beacon_block_root: Some(B256::ZERO),
                },
                transactions: Some(vec![info_tx]),
                no_tx_pool: Some(false),
                gas_limit,
                eip_1559_params: None,
            }),
        )
        .await?;
    Ok(fcu_response)
}

#[async_trait::async_trait]
impl AdvanceChain for AuthProviderOp {
    /// Advance the chain by calling `engine_forkchoiceUpdated` (FCU) and `engine_newPayload` methods.
    async fn advance_chain(&self, block_time_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
        println!("[OP] advancing chain {block_time_secs}s...");
        let engine_client = &self.inner;
        let block = engine_client
            .latest()
            .await?
            .expect("latest block not found");

        //
        // First FCU call: call with same hash for parent and new head
        //
        let res = call_fcu_default(
            engine_client,
            block.header.hash,
            block.header.hash,
            Some(block.header.timestamp + block_time_secs),
            Some(block.header.gas_limit),
        )
        .await?;
        println!("[OP] FCU call sent. Payload ID: {:?}", res.payload_id);
        let payload_id = res.payload_id.ok_or("need payload ID")?;

        // wait for builder to build
        tokio::time::sleep(Duration::from_millis(1000)).await;

        //
        // getPayload w/ new ID
        //
        let payload = engine_client.get_payload_v3(payload_id).await?;

        //
        // newPayload w/ fresh payload from target
        //
        let _res = engine_client
            .new_payload(payload.execution_payload.to_owned(), vec![], B256::ZERO)
            .await?;

        //
        // second FCU call: call with updated block head from new payload
        //
        let res = call_fcu_default(
            engine_client,
            block.header.hash,
            payload
                .execution_payload
                .payload_inner
                .payload_inner
                .block_hash,
            None,
            Some(block.header.gas_limit),
        )
        .await?;
        println!("[OP] FCU call sent. Payload ID: {:?}", res.payload_id);

        Ok(())
    }
}
