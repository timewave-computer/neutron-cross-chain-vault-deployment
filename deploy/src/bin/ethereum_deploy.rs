use types::ethereum_config::{
    EthereumAccounts, EthereumDenoms, EthereumLibraries, EthereumStrategyConfig,
};

fn main() {
    let denoms = EthereumDenoms {
        wbtc: "0xWBTC_ADDR...".to_string(),
    };

    let accounts = EthereumAccounts {
        deposit: "0xDeposit_account...".to_string(),
    };

    let libraries = EthereumLibraries {
        one_way_vault: "0xone_way_vault...".to_string(),
        eureka_forwarder: "0xeureka_fwd...".to_string(),
    };

    let _eth_cfg = EthereumStrategyConfig {
        rpc_url: "https://...".to_string(),
        mnemonic: "racoon racoon racoon racoon racoon racoon...".to_string(),
        authorizations: "0xauthorizations...".to_string(),
        processor: "0xprocessor...".to_string(),
        denoms,
        accounts,
        libraries,
    };
}
