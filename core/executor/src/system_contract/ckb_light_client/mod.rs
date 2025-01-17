mod abi;
mod store;

pub use abi::ckb_light_client_abi;

use std::sync::atomic::{AtomicBool, Ordering};

use ethers::abi::AbiDecode;

use protocol::traits::{ApplyBackend, ExecutorAdapter};
use protocol::types::{SignedTransaction, TxResp, H160, H256};
use protocol::ProtocolResult;

use crate::system_contract::ckb_light_client::store::CkbLightClientStore;
use crate::system_contract::utils::{succeed_resp, update_states};
use crate::system_contract::{system_contract_address, SystemContract};
use crate::{exec_try, system_contract_struct, CURRENT_HEADER_CELL_ROOT};

pub const CKB_LIGHT_CLIENT_CONTRACT_ADDRESS: H160 = system_contract_address(0x2);
static ALLOW_READ: AtomicBool = AtomicBool::new(false);

system_contract_struct!(CkbLightClientContract);

impl<Adapter: ExecutorAdapter + ApplyBackend> SystemContract<Adapter>
    for CkbLightClientContract<Adapter>
{
    const ADDRESS: H160 = CKB_LIGHT_CLIENT_CONTRACT_ADDRESS;

    fn exec_(&self, adapter: &mut Adapter, tx: &SignedTransaction) -> TxResp {
        let sender = tx.sender;
        let tx = &tx.transaction.unsigned;
        let tx_data = tx.data();
        let gas_limit = *tx.gas_limit();

        let root = CURRENT_HEADER_CELL_ROOT.with(|r| *r.borrow());
        let mut store = exec_try!(
            CkbLightClientStore::new(root),
            gas_limit,
            "[ckb light client] init ckb light client mpt"
        );

        let call_abi = exec_try!(
            ckb_light_client_abi::CkbLightClientContractCalls::decode(tx_data),
            gas_limit,
            "[ckb light client] invalid tx data"
        );

        match call_abi {
            ckb_light_client_abi::CkbLightClientContractCalls::SetState(data) => {
                ALLOW_READ.store(data.allow_read, Ordering::Relaxed);
            }
            ckb_light_client_abi::CkbLightClientContractCalls::Update(data) => {
                exec_try!(
                    store.update(data),
                    gas_limit,
                    "[ckb light client] update error:"
                );
            }
            ckb_light_client_abi::CkbLightClientContractCalls::Rollback(data) => {
                exec_try!(
                    store.rollback(data),
                    gas_limit,
                    "[ckb light client] update error:"
                );
            }
        }

        update_states(adapter, sender, Self::ADDRESS);
        succeed_resp(gas_limit)
    }
}

#[derive(Default)]
pub(crate) struct CkbHeaderReader;

/// These methods are provide for interoperation module to get CKB headers.
impl CkbHeaderReader {
    pub fn get_header_by_block_hash(
        &self,
        root: H256,
        block_hash: &H256,
    ) -> ProtocolResult<Option<ckb_light_client_abi::Header>> {
        let store = CkbLightClientStore::new(root)?;
        store.get_header(&block_hash.0)
    }

    pub fn get_raw(&self, root: H256, key: &[u8]) -> ProtocolResult<Option<Vec<u8>>> {
        let store = CkbLightClientStore::new(root)?;
        let ret = store.trie.get(key)?.map(Into::into);
        Ok(ret)
    }

    #[cfg(test)]
    pub fn allow_read(&self) -> bool {
        ALLOW_READ.load(Ordering::Relaxed)
    }
}
