use cosmwasm_std::Uint128;
use types::neutron_config::{
    IcaAccount, NeutronAccounts, NeutronDenoms, NeutronLibraries, NeutronStrategyConfig,
};

fn main() {
    let denoms = NeutronDenoms {
        wbtc: "ibc/wbtc...".to_string(),
        ntrn: "untrn".to_string(),
        supervault_lp: "factory/neutron1.../supervault".to_string(),
    };

    let accounts = NeutronAccounts {
        deposit: "neutron1deposit...".to_string(),
        mars: "neutron1mars...".to_string(),
        supervault: "neutron1supervault...".to_string(),
        settlement: "neutron1settlement...".to_string(),
        gaia_ica: IcaAccount {
            library_account: "neutron1ica...".to_string(),
            remote_addr: "cosmos1ica...".to_string(),
        },
    };

    let libraries = NeutronLibraries {
        clearing: "neutron1clearing...".to_string(),
        mars_lending: "neutron1mars_lending...".to_string(),
        supervaults_depositor: "neutron1supervaults_depositor...".to_string(),
        deposit_forwarder: "neutron1deposit_fwd...".to_string(),
        ica_ibc_transfer: "neutron1ica_ibc_transfer...".to_string(),
    };

    let _neutron_cfg = NeutronStrategyConfig {
        grpc_url: "https://0.0.0.0".to_string(),
        grpc_port: "12345".to_string(),
        chain_id: "neutron-1".to_string(),
        mnemonic: "racoon racoon racoon racoon racoon racoon".to_string(),
        mars_pool: "neutron1mars...".to_string(),
        supervault: "neutron1supervault...".to_string(),
        denoms,
        accounts,
        libraries,
        min_ibc_fee: Uint128::one(),
        authorizations: "neutron1authorizations...".to_string(),
        processor: "neutron1processor...".to_string(),
    };
}
