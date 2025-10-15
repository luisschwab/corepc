// SPDX-License-Identifier: CC0-1.0

//! Tests for methods found under the `== Wallet ==` section of the API docs.

#![allow(non_snake_case)] // Test names intentionally use double underscore.
#![allow(unused_imports)] // Some imports are only used in specific versions.

use std::collections::BTreeMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use bitcoin::address::{self, Address, KnownHrp, NetworkChecked};
use bitcoin::bip32::{Xpriv, Xpub};
use bitcoin::{
    amount, hex, key, psbt, secp256k1, sign_message, Amount, CompressedPublicKey, FeeRate, Network,
    PrivateKey, PublicKey,
};
use integration_test::{Node, NodeExt as _, Wallet};
use node::vtype::*; // All the version specific types.
#[cfg(not(feature = "v20_and_below"))]
use node::ImportDescriptorsRequest;
use node::{
    mtype, AddressType, ImportMultiRequest, ImportMultiScriptPubKey, ImportMultiTimestamp,
    WalletCreateFundedPsbtInput,
};

#[test]
fn wallet__abandon_transaction() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    let mining_addr = node.client.new_address().expect("newaddress");
    let json: GenerateToAddress =
        node.client.generate_to_address(101, &mining_addr).expect("generatetoaddress");
    let block_hashes = json.into_model();

    let block_hash = block_hashes.expect("blockhash").0[0];

    let dest_addr = node.client.new_address().expect("newaddress");
    let amount = bitcoin::Amount::from_sat(1_000_000);

    let txid = node
        .client
        .send_to_address_rbf(&dest_addr, amount)
        .expect("sendtoaddressrbf")
        .txid()
        .expect("txid");

    node.client.invalidate_block(block_hash).expect("invalidateblock");

    let _: () = node.client.abandon_transaction(txid).expect("abandontransaction");
}

#[test]
fn wallet__abort_rescan() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    let json: AbortRescan = node.client.abort_rescan().expect("abortrescan");
    assert!(!json.0); // No rescan running, abort should return false
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__add_multisig_address__modelled() {
    let nrequired = 2;

    let node = match () {
        #[cfg(feature = "v22_and_below")]
        () => Node::with_wallet(Wallet::Default, &[]),
        #[cfg(not(feature = "v22_and_below"))]
        () => {
            let node = Node::with_wallet(Wallet::None, &["-deprecatedrpc=create_bdb"]);
            node.client.create_legacy_wallet("wallet_name").expect("createlegacywallet");
            node
        }
    };

    let addr1 = node.client.new_address().expect("new_address");
    let addr2 = node.client.new_address().expect("new_address");

    let json: AddMultisigAddress = node
        .client
        .add_multisig_address_with_addresses(nrequired, vec![addr1, addr2])
        .expect("addmultisigaddress");

    let model: Result<mtype::AddMultisigAddress, AddMultisigAddressError> = json.into_model();
    model.unwrap();
}

#[test]
fn wallet__backup_wallet() { backup_and_restore_wallet() }

fn backup_and_restore_wallet() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    let file_path = integration_test::random_tmp_file();

    let _: () = node.client.backup_wallet(&file_path).expect("backupwallet");
    assert!(file_path.exists(), "Backup file should exist at destination");
    assert!(file_path.is_file(), "Backup destination should be a file");

    // Restore wallet only available for v23 and above.
    #[cfg(not(feature = "v22_and_below"))]
    {
        let wallet_name = "test_wallet";
        let node2 = Node::with_wallet(Wallet::None, &[]);
        let restored_wallet: RestoreWallet =
            node2.client.restore_wallet(wallet_name, &file_path).expect("restorewallet");
        assert_eq!(restored_wallet.name, wallet_name);
    }

    fs::remove_file(&file_path).expect("removefile");
}

#[test]
fn wallet__bump_fee__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    let address = node.client.new_address().expect("failed to create new address");
    let _ = node.client.generate_to_address(101, &address).expect("generatetoaddress");

    let txid = node
        .client
        .send_to_address_rbf(&address, Amount::from_sat(10_000))
        .expect("sendtoaddress")
        .txid()
        .unwrap();

    let json: BumpFee = node.client.bump_fee(txid).expect("bumpfee");
    let model: Result<mtype::BumpFee, BumpFeeError> = json.into_model();
    model.unwrap();
}

#[test]
fn wallet__create_wallet__modelled() {
    // Implicitly tests `createwallet` because we create the default wallet.
    let _ = Node::with_wallet(Wallet::Default, &[]);
}

#[test]
#[cfg(not(feature = "v27_and_below"))]
fn wallet__create_wallet_descriptor() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    // BIP32 HD xprv/xpub for the creation of a descriptor with a private key that is in the wallet.
    let secp = secp256k1::Secp256k1::new();
    let seed = [0u8; 32];
    let xprv = Xpriv::new_master(Network::Regtest, &seed).unwrap();
    let xpub = Xpub::from_priv(&secp, &xprv);
    let hdkey = xpub.to_string();

    // Import the private key into the wallet.
    let privkey = bitcoin::PrivateKey {
        compressed: true,
        network: Network::Regtest.into(),
        inner: xprv.private_key,
    };
    let wif = privkey.to_wif();
    let raw_descriptor = format!("wpkh({})", wif);
    let info = node.client.get_descriptor_info(&raw_descriptor).expect("get_descriptor_info");
    let descriptor = format!("{}#{}", raw_descriptor, info.checksum);

    let import_req = ImportDescriptorsRequest::new(descriptor, 0);
    node.client.import_descriptors(&[import_req]).expect("importdescriptors");

    let json: CreateWalletDescriptor =
        node.client.create_wallet_descriptor("bech32", &hdkey).expect("createwalletdescriptor");

    // Check that a SigWit descriptor was created.
    let prefix = &json.descriptors[0][0..4];
    assert_eq!(prefix, "wpkh");
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__dump_priv_key__modelled() {
    // As of Core v23 the default wallet is an native descriptor wallet which does not
    // support dumping private keys. Legacy wallets are supported upto v25 it seems.
    #[cfg(all(feature = "v25_and_below", not(feature = "v22_and_below")))]
    {
        let node = Node::with_wallet(Wallet::None, &[]);

        node.client.create_legacy_wallet("legacy_wallet").expect("legacy create_wallet");
        let address = node
            .client
            .get_new_address(Some("label"), Some(AddressType::Legacy))
            .expect("legacy get_new_address");
        let model: Result<mtype::GetNewAddress, address::ParseError> = address.into_model();
        let address = model.unwrap().0.assume_checked();

        let json: DumpPrivKey = node.client.dump_priv_key(&address).expect("dumpprivkey");
        let model: Result<mtype::DumpPrivKey, key::FromWifError> = json.into_model();
        model.unwrap();
    }

    #[cfg(feature = "v22_and_below")]
    {
        let node = Node::with_wallet(Wallet::Default, &[]);
        let address = node.client.new_address().expect("failed to get new address");

        let json: DumpPrivKey = node.client.dump_priv_key(&address).expect("dumpprivkey");
        let model: Result<mtype::DumpPrivKey, key::FromWifError> = json.into_model();
        model.unwrap();
    }
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__dump_wallet() {
    // As of Core v23 the default wallet is an native descriptor wallet which does not
    // support dumping private keys. Legacy wallets are supported upto v25 it seems.
    #[cfg(all(feature = "v25_and_below", not(feature = "v22_and_below")))]
    {
        let node = Node::with_wallet(Wallet::None, &[]);

        node.client.create_legacy_wallet("legacy_wallet").expect("legacy create_wallet");
        let out = integration_test::random_tmp_file();

        let _: DumpWallet = node.client.dump_wallet(&out).expect("dumpwallet");
    }

    #[cfg(feature = "v22_and_below")]
    {
        let node = Node::with_wallet(Wallet::Default, &[]);
        let out = integration_test::random_tmp_file();

        let _: DumpWallet = node.client.dump_wallet(&out).expect("dumpwallet");
    }
}

#[test]
fn wallet__encrypt_wallet() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    let _: EncryptWallet = node.client.encrypt_wallet("test-passphrase").expect("encryptwallet");
}

#[test]
fn wallet__get_addresses_by_label__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    let label = "some-label";
    let addr = node.client.new_address_with_label(label).expect("failed to get new address");

    let json: GetAddressesByLabel =
        node.client.get_addresses_by_label(label).expect("getaddressesbylabel");
    let model: Result<mtype::GetAddressesByLabel, address::ParseError> = json.into_model();
    let map = model.unwrap();

    // sanity checks.
    assert!(!map.0.is_empty());
    assert!(map.0.contains_key(&addr));
}

#[test]
fn wallet__get_address_info__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    // Test an address with a label.
    let label_name = "test-label";
    let addr = node.client.new_address_with_label(label_name).unwrap().assume_checked();
    let json: GetAddressInfo = node.client.get_address_info(&addr).expect("getaddressinfo legacy");
    let model: Result<mtype::GetAddressInfo, GetAddressInfoError> = json.into_model();
    let address_info = model.unwrap();
    assert_eq!(address_info.address.assume_checked(), addr);
    assert_eq!(address_info.labels[0], label_name);

    // Test a SegWit address with embedded information.
    let addr_p2sh = node.client.new_address_with_type(AddressType::P2shSegwit).unwrap();
    let json: GetAddressInfo =
        node.client.get_address_info(&addr_p2sh).expect("getaddressinfo p2sh-segwit");
    let model: Result<mtype::GetAddressInfo, GetAddressInfoError> = json.into_model();
    let address_info = model.unwrap();
    let embedded = address_info.embedded.unwrap();
    assert_eq!(address_info.address.assume_checked(), addr_p2sh);
    assert_eq!(address_info.script.unwrap(), mtype::ScriptType::WitnessV0KeyHash);
    assert!(embedded.address.is_valid_for_network(Network::Regtest));

    // Test a Bech32 address.
    let addr_bech32 = node.client.new_address_with_type(AddressType::Bech32).unwrap();
    let json: GetAddressInfo =
        node.client.get_address_info(&addr_bech32).expect("getaddressinfo bech32");
    let model: Result<mtype::GetAddressInfo, GetAddressInfoError> = json.into_model();
    let address_info = model.unwrap();
    assert_eq!(address_info.address.assume_checked(), addr_bech32);
}

#[test]
fn wallet__get_balance__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    let json: GetBalance = node.client.get_balance().expect("getbalance");
    let model: Result<mtype::GetBalance, amount::ParseAmountError> = json.into_model();
    model.unwrap();

    // Check non-zero balance just for giggles.
    node.fund_wallet();
    let json: GetBalance = node.client.get_balance().expect("getbalance");
    let model: Result<mtype::GetBalance, amount::ParseAmountError> = json.into_model();
    model.unwrap();
}

#[test]
#[cfg(not(feature = "v18_and_below"))]
fn wallet__get_balances() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();

    let json: GetBalances = node.client.get_balances().expect("getbalances");
    let model: Result<mtype::GetBalances, GetBalancesError> = json.into_model();
    model.unwrap();
}

#[test]
#[cfg(not(feature = "v27_and_below"))]
fn wallet__get_hd_keys__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    let json: GetHdKeys = node.client.get_hd_keys().expect("gethdkeys");
    let model: Result<mtype::GetHdKeys, GetHdKeysError> = json.into_model();
    let hdkey = model.unwrap().0;

    let descriptor_type = hdkey[0].descriptors[0].descriptor[..3].to_string();
    assert_eq!(descriptor_type, "pkh");
}

#[test]
fn wallet__get_new_address__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    // Implicitly tests `getnewaddress`.
    let _ = node.client.new_address().unwrap();

    // Exhaustively test address types with helper.
    let _ = node.client.new_address_with_type(AddressType::Legacy).unwrap();
    let _ = node.client.new_address_with_type(AddressType::P2shSegwit).unwrap();
    let _ = node.client.new_address_with_type(AddressType::Bech32).unwrap();
}

#[test]
fn wallet__get_raw_change_address__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    let json: GetRawChangeAddress =
        node.client.get_raw_change_address().expect("getrawchangeaddress");
    let model: Result<mtype::GetRawChangeAddress, address::ParseError> = json.into_model();
    model.unwrap();
}

#[test]
fn wallet__get_received_by_address__modelled() {
    let amount = Amount::from_sat(10_000);

    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();
    let address = node.client.new_address().expect("failed to create new address");

    let _txid =
        node.client.send_to_address(&address, amount).expect("sendtoaddress").txid().unwrap();
    node.mine_a_block();

    let json: GetReceivedByAddress =
        node.client.get_received_by_address(&address).expect("getreceivedbyaddress");
    let model: Result<mtype::GetReceivedByAddress, amount::ParseAmountError> = json.into_model();
    let received_by_address = model.unwrap();

    assert_eq!(received_by_address.0, amount);
}

#[test]
#[cfg(not(feature = "v17"))]
fn wallet__get_received_by_label__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();
    let label = "test-label";

    // Send some coins to the label
    let amount = Amount::from_sat(10_000);
    let address = node.client.new_address_with_label(label).unwrap().assume_checked();
    let _ = node.client.send_to_address(&address, amount).unwrap();
    node.mine_a_block();

    let json: GetReceivedByLabel =
        node.client.get_received_by_label(label).expect("getreceivedbylabel");
    let model: Result<mtype::GetReceivedByLabel, amount::ParseAmountError> = json.into_model();
    let received = model.unwrap();
    assert_eq!(received.0, amount);
}

#[test]
fn wallet__get_transaction__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();
    let address = node.client.new_address().expect("failed to create new address");

    let txid = node
        .client
        .send_to_address(&address, Amount::from_sat(10_000))
        .expect("sendtoaddress")
        .txid()
        .unwrap();

    let json: GetTransaction = node.client.get_transaction(txid).expect("gettransaction");
    let model: Result<mtype::GetTransaction, GetTransactionError> = json.into_model();
    model.unwrap();
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__get_unconfirmed_balance__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    let json: GetUnconfirmedBalance =
        node.client.get_unconfirmed_balance().expect("getunconfirmedbalance");
    let model: Result<mtype::GetUnconfirmedBalance, amount::ParseAmountError> = json.into_model();
    model.unwrap();
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__get_wallet_info__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.mine_a_block();

    let json: GetWalletInfo = node.client.get_wallet_info().expect("getwalletinfo");
    let model: Result<mtype::GetWalletInfo, GetWalletInfoError> = json.into_model();
    let wallet_info = model.unwrap();

    assert!(!wallet_info.wallet_name.is_empty());

    #[cfg(not(feature = "v18_and_below"))]
    {
        assert!(wallet_info.avoid_reuse.is_some());
        assert!(wallet_info.scanning.is_some());
    }

    #[cfg(not(feature = "v25_and_below"))]
    {
        let last_processed =
            wallet_info.last_processed_block.as_ref().expect("last_processed_block");
        let best_hash = node.client.best_block_hash().expect("best_block_hash");
        assert_eq!(last_processed.hash, best_hash);
    }
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__import_address() {
    let node = match () {
        #[cfg(feature = "v22_and_below")]
        () => Node::with_wallet(Wallet::Default, &[]),
        #[cfg(not(feature = "v22_and_below"))]
        () => {
            let node = Node::with_wallet(Wallet::None, &["-deprecatedrpc=create_bdb"]);
            node.client.create_legacy_wallet("wallet_name").expect("createlegacywallet");
            node
        }
    };

    let privkey =
        PrivateKey::from_wif("cVt4o7BGAig1UXywgGSmARhxMdzP5qvQsxKkSsc1XEkw3tDTQFpy").unwrap();

    // Derive the address from the private key
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let pubkey = privkey.public_key(&secp);
    let addr = bitcoin::Address::p2pkh(pubkey, privkey.network);

    let _: () = node.client.import_address(&addr).expect("importaddress");
}

#[test]
#[cfg(not(feature = "v20_and_below"))]
fn wallet__import_descriptors() {
    let node = Node::with_wallet(Wallet::None, &[]);
    let wallet_name = "desc_wallet";

    #[cfg(feature = "v22_and_below")]
    node.client.create_descriptor_wallet(wallet_name).expect("create descriptor wallet");

    // v23 onwards uses descriptor wallets by default.
    #[cfg(not(feature = "v22_and_below"))]
    node.client.create_wallet(wallet_name).expect("create wallet");

    node.fund_wallet();

    // 1. Get the current time
    let start_time =
        SystemTime::now().duration_since(UNIX_EPOCH).expect("failed to get current time").as_secs();

    // 2. Use a known private key, derive the address from it and send some coins to it.
    let privkey =
        PrivateKey::from_wif("cVt4o7BGAig1UXywgGSmARhxMdzP5qvQsxKkSsc1XEkw3tDTQFpy").unwrap();
    let secp = secp256k1::Secp256k1::new();
    let pubkey = privkey.public_key(&secp);
    let address = Address::p2wpkh(&CompressedPublicKey(pubkey.inner), KnownHrp::Regtest);
    let amount = Amount::from_sat(10_000);
    let _txid = node.client.send_to_address(&address, amount).expect("sendtoaddress");

    // 3. Get the descriptor from the private key.
    let raw_descriptor = format!("wpkh({})", privkey.to_wif());
    let info = node.client.get_descriptor_info(&raw_descriptor).expect("get_descriptor_info");
    let descriptor = format!("{}#{}", raw_descriptor, info.checksum);

    // 4. Mine 100 blocks
    let mining_address = node.client.new_address().expect("failed to get mining address");
    let _blocks = node.client.generate_to_address(100, &mining_address).expect("generatetoaddress");

    // 5. Scan for the descriptor using the time from (1)
    let request = ImportDescriptorsRequest::new(descriptor, start_time);
    let result: ImportDescriptors =
        node.client.import_descriptors(&[request]).expect("importdescriptors");
    assert_eq!(result.0.len(), 1, "should have exactly one import result");
    assert!(result.0[0].success);
}

#[test]
fn wallet__import_pruned_funds() {
    let node = Node::with_wallet(Wallet::Default, &["-txindex"]);
    node.fund_wallet();

    let (_, tx) = node.create_mined_transaction();
    let txid = tx.compute_txid();

    let raw_tx = node.client.get_raw_transaction(txid).expect("getrawtransaction");
    let tx_out_proof = node.client.get_tx_out_proof(&[txid]).expect("gettxoutproof");

    let _: () =
        node.client.import_pruned_funds(&raw_tx.0, &tx_out_proof).expect("importprunedfunds");
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__import_wallet() {
    let node = match () {
        #[cfg(feature = "v22_and_below")]
        () => Node::with_wallet(Wallet::Default, &[]),
        #[cfg(not(feature = "v22_and_below"))]
        () => {
            let node = Node::with_wallet(Wallet::None, &["-deprecatedrpc=create_bdb"]);
            node.client.create_legacy_wallet("wallet_name").expect("createlegacywallet");
            node
        }
    };

    node.client.new_address().expect("newaddress");
    let dump_file_path = integration_test::random_tmp_file();

    node.client.dump_wallet(&dump_file_path).expect("dumpwallet");
    assert!(dump_file_path.exists());

    let _: () = node.client.import_wallet(&dump_file_path).expect("importwallet");
}

#[test]
fn wallet__keypool_refill() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    let _: () = node.client.key_pool_refill().expect("keypoolrefill");
}

#[test]
fn wallet__list_address_groupings__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();

    let address = node.client.new_address().expect("failed to create new address");
    let amount = Amount::from_sat(10_000);
    node.client.send_to_address(&address, amount).expect("sendtoaddress").txid().unwrap();
    node.mine_a_block();

    let json: ListAddressGroupings =
        node.client.list_address_groupings().expect("listaddressgroupings");
    let model: Result<mtype::ListAddressGroupings, ListAddressGroupingsError> = json.into_model();
    let groupings = model.unwrap();

    assert!(!groupings.0.is_empty());
}

#[test]
fn wallet__list_labels__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    let label = "list-label-test";
    let _ = node.client.new_address_with_label(label).expect("newaddress");

    let json: ListLabels = node.client.list_labels().expect("listlabels");

    assert!(json.0.iter().any(|s| s == label));
}

#[test]
#[cfg(not(feature = "v17"))]
fn wallet__list_received_by_label__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();
    let label = "test-label";

    // Send some coins to the label
    let amount = Amount::from_sat(10_000);
    let address = node.client.new_address_with_label(label).unwrap().assume_checked();
    let _ = node.client.send_to_address(&address, amount).unwrap();
    node.mine_a_block();

    let json: ListReceivedByLabel =
        node.client.list_received_by_label().expect("listreceivedbylabel");
    let model: Result<mtype::ListReceivedByLabel, ListReceivedByLabelError> = json.into_model();
    let received_by_label = model.unwrap();
    assert!(received_by_label.0.iter().any(|item| item.label == label));
}

#[test]
fn wallet__list_received_by_address__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();
    let address = node.client.new_address().expect("failed to create new address");
    let amount = Amount::from_sat(10_000);
    let _ = node.client.send_to_address(&address, amount).expect("sendtoaddress");
    node.mine_a_block();

    let json: ListReceivedByAddress =
        node.client.list_received_by_address().expect("listreceivedbyaddress");
    let model: Result<mtype::ListReceivedByAddress, ListReceivedByAddressError> = json.into_model();
    let received_by_address = model.unwrap();

    let unchecked_addr = address.as_unchecked();
    assert!(received_by_address.0.iter().any(|item| &item.address == unchecked_addr));
}

#[test]
fn wallet__list_since_block__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();
    let addr = node.client.new_address().expect("newaddress");
    let amount = Amount::from_sat(5_000);
    node.client.send_to_address(&addr, amount).expect("sendtoaddress");
    node.mine_a_block();

    let json: ListSinceBlock = node.client.list_since_block().expect("listsinceblock");
    let model: Result<mtype::ListSinceBlock, ListSinceBlockError> = json.into_model();
    let list_since_block = model.unwrap();

    let first_tx: mtype::TransactionItem = list_since_block.transactions[0].clone();
    assert_eq!(first_tx.txid.unwrap().to_string().len(), 64);
}

#[test]
fn wallet__list_transactions__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    node.fund_wallet();
    let addr = node.client.new_address().expect("newaddress");
    let amount = Amount::from_sat(5_000);
    node.client.send_to_address(&addr, amount).expect("sendtoaddress");
    node.mine_a_block();

    let json: ListTransactions = node.client.list_transactions().expect("listtransactions");
    let model: Result<mtype::ListTransactions, TransactionItemError> = json.into_model();
    let list_transactions = model.unwrap();

    let first_tx: mtype::TransactionItem = list_transactions.0[0].clone();
    assert_eq!(first_tx.txid.unwrap().to_string().len(), 64);
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__import_multi() {
    let node = match () {
        #[cfg(feature = "v22_and_below")]
        () => Node::with_wallet(Wallet::Default, &[]),
        #[cfg(not(feature = "v22_and_below"))]
        () => {
            let node = Node::with_wallet(Wallet::None, &["-deprecatedrpc=create_bdb"]);
            node.client.create_legacy_wallet("wallet_name").expect("createlegacywallet");
            node
        }
    };

    let dummy_script_hex = "76a914aabbccddeeff00112233445566778899aabbccdd88ac";
    let addr = node.client.new_address().expect("newaddress");
    let dummy_desc =
        "pkh(02c6047f9441ed7d6d3045406e95c07cd85a2a0e5c1e507a7a7e3d2f0d6c3d8ef8)#tp9h0863";

    // Uses scriptPubKey (valid): success - true, without warnings nor error.
    // NOTE: On v17, use a wallet-generated address (not raw script)
    // to ensure import succeeds, since the wallet already knows the key.
    let req1 = ImportMultiRequest {
        descriptor: None,
        script_pubkey: Some(ImportMultiScriptPubKey::Script(dummy_script_hex.to_string())),
        timestamp: ImportMultiTimestamp::Now,
    };

    // Uses an address (valid): success - false, with JSON-RPC error.
    let req2 = ImportMultiRequest {
        descriptor: None,
        script_pubkey: Some(ImportMultiScriptPubKey::Address { address: addr.to_string() }),
        timestamp: ImportMultiTimestamp::Now,
    };

    // Uses descriptor (valid): success - true
    // on v18 onwards, it will return a watch-only warning.
    // NOTE: Works only for v18 onwards, as v17 doesn't support descriptors.
    let req3 = ImportMultiRequest {
        descriptor: Some(dummy_desc.to_string()),
        script_pubkey: None,
        timestamp: ImportMultiTimestamp::Time(1_700_000_000),
    };

    let json: ImportMulti = node.client.import_multi(&[req1, req2, req3]).expect("importmulti");

    #[cfg(not(feature = "v17"))]
    {
        // result of req1: should succeed, no error, no warning.
        // just any random script doesn't work with v17.
        assert!(json.0[0].success);
        assert!(json.0[0].error.is_none());

        // result of req3: should succeed, with warning for v18 onwards
        assert!(json.0[2].success);
        assert!(json.0[2].error.is_none());
        assert!(json.0[2].warnings.is_some());
    }

    // result of req2: should fail with error (wallet already contains privkey for address/script)
    assert!(!json.0[1].success);
    assert!(json.0[1].error.is_some());
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__import_privkey() {
    let node = match () {
        #[cfg(feature = "v22_and_below")]
        () => Node::with_wallet(Wallet::Default, &[]),
        #[cfg(not(feature = "v22_and_below"))]
        () => {
            let node = Node::with_wallet(Wallet::None, &["-deprecatedrpc=create_bdb"]);
            node.client.create_legacy_wallet("wallet_name").expect("createlegacywallet");
            node
        }
    };

    let privkey =
        PrivateKey::from_wif("cVt4o7BGAig1UXywgGSmARhxMdzP5qvQsxKkSsc1XEkw3tDTQFpy").unwrap();

    let _: () = node.client.import_privkey(&privkey).expect("importprivkey");
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__import_pubkey() {
    let node = match () {
        #[cfg(feature = "v22_and_below")]
        () => Node::with_wallet(Wallet::Default, &[]),
        #[cfg(not(feature = "v22_and_below"))]
        () => {
            let node = Node::with_wallet(Wallet::None, &["-deprecatedrpc=create_bdb"]);
            node.client.create_legacy_wallet("wallet_name").expect("createlegacywallet");
            node
        }
    };

    let pubkey = "02ff12471208c14bd580709cb2358d98975247d8765f92bc25eab3b2763ed605f8"
        .parse::<PublicKey>()
        .unwrap();

    let _: () = node.client.import_pubkey(&pubkey).expect("importpubkey");
}

#[test]
#[cfg(not(feature = "v21_and_below"))]
fn wallet__list_descriptors() {
    let node = Node::with_wallet(Wallet::None, &[]);
    let wallet_name = "desc_wallet";

    #[cfg(feature = "v22_and_below")]
    node.client.create_descriptor_wallet(wallet_name).expect("create descriptor wallet");

    // v23 onwards uses descriptor wallets by default.
    #[cfg(not(feature = "v22_and_below"))]
    node.client.create_wallet(wallet_name).expect("create wallet");

    let json: ListDescriptors = node.client.list_descriptors().expect("listdescriptors");

    let has_descriptor = json.descriptors.iter().any(|desc_info| {
        desc_info.descriptor.starts_with("wpkh(") || desc_info.descriptor.starts_with("pkh(")
    });
    assert!(has_descriptor, "No standard descriptors found in listdescriptors result");
}

#[test]
fn wallet__list_lock_unspent__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();

    let json: ListUnspent = node.client.list_unspent().expect("listunspent");
    let utxos: mtype::ListUnspent = json.into_model().unwrap();
    let txid = utxos.0[0].txid;
    let vout = utxos.0[0].vout;
    node.client.lock_unspent(&[(txid, vout)]).expect("lockunspent");

    let json: ListLockUnspent = node.client.list_lock_unspent().expect("listlockunspent");
    let model: Result<mtype::ListLockUnspent, ListLockUnspentItemError> = json.into_model();
    let lock_unspent = model.unwrap();

    assert!(lock_unspent.0.iter().any(|o| o.txid == txid && o.vout == vout));
}

#[test]
fn wallet__list_unspent__modelled() {
    let node = match () {
        #[cfg(feature = "v17")]
        () => Node::with_wallet(Wallet::Default, &["-deprecatedrpc=accounts"]),
        #[cfg(not(feature = "v17"))]
        () => Node::with_wallet(Wallet::Default, &[]),
    };

    node.fund_wallet();

    let json: ListUnspent = node.client.list_unspent().expect("listunspent");
    let model: Result<mtype::ListUnspent, ListUnspentItemError> = json.into_model();
    model.unwrap();
}

#[test]
#[cfg(all(feature = "v29_and_below", not(feature = "v17")))]
fn wallet__list_wallet_dir() {
    let wallet_name = "test-wallet";
    let node = Node::with_wallet(Wallet::None, &[]);
    node.client.create_wallet(wallet_name).expect("failed to create wallet");

    let wallet_dir = node.client.list_wallet_dir().expect("listwalletdir");
    let wallet_names: Vec<_> = wallet_dir.wallets.iter().map(|w| &w.name).collect();

    assert!(wallet_names.iter().any(|w| *w == wallet_name));
}

#[test]
fn wallet__list_wallets__modelled() {
    let node = Node::with_wallet(Wallet::None, &[]);

    let wallet_1 = "test_wallet_1";
    let wallet_2 = "test_wallet_2";
    node.client.create_wallet(wallet_1).expect("createwallet w1");
    node.client.create_wallet(wallet_2).expect("createwallet w2");

    let json: ListWallets = node.client.list_wallets().expect("listwallets");

    assert!(json.0.iter().any(|w| w == wallet_1));
    assert!(json.0.iter().any(|w| w == wallet_2));
}

#[test]
fn wallet__load_wallet__modelled() { create_load_unload_wallet(); }

#[test]
fn wallet__lock_unspent() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();

    let json: ListUnspent = node.client.list_unspent().expect("listunspent");
    let utxos: mtype::ListUnspent = json.into_model().unwrap();
    let txid = utxos.0[0].txid;
    let vout = utxos.0[0].vout;

    let locked: LockUnspent = node.client.lock_unspent(&[(txid, vout)]).expect("lockunspent");
    assert!(locked.0, "lock_unspent");

    let unlocked: LockUnspent = node.client.unlock_unspent(&[(txid, vout)]).expect("unlockunspent");
    assert!(unlocked.0, "unlock_unspent");
}

#[test]
#[cfg(all(feature = "v29_and_below", not(feature = "v23_and_below")))]
fn wallet__migrate_wallet() {
    let node = Node::with_wallet(Wallet::None, &["-deprecatedrpc=create_bdb"]);
    let wallet_name = "legacy_wallet";
    node.client.create_legacy_wallet(wallet_name).expect("createlegacywallet");

    let json: MigrateWallet = node.client.migrate_wallet(wallet_name).expect("migratewallet");

    assert_eq!(json.wallet_name, wallet_name);
}

#[test]
#[cfg(all(feature = "v29_and_below", not(feature = "v22_and_below")))]
fn wallet__new_keypool() {
    let node = Node::with_wallet(Wallet::None, &["-deprecatedrpc=create_bdb"]);
    node.client.create_legacy_wallet("legacy_wallet").expect("createlegacywallet");
    let _: () = node.client.new_keypool().expect("newkeypool");
}

#[test]
#[cfg(not(feature = "v20_and_below"))]
fn wallet__psbt_bump_fee__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    let address = node.client.new_address().expect("failed to create new address");
    let _ = node.client.generate_to_address(101, &address).expect("generatetoaddress");

    let txid = node
        .client
        .send_to_address_rbf(&address, Amount::from_sat(10_000))
        .expect("sendtoaddress")
        .txid()
        .unwrap();

    let json: PsbtBumpFee = node.client.psbt_bump_fee(&txid).expect("psbtbumpfee");
    let model: Result<mtype::PsbtBumpFee, PsbtBumpFeeError> = json.into_model();
    model.unwrap();
}

#[test]
fn wallet__remove_pruned_funds() {
    let node = Node::with_wallet(Wallet::Default, &["-txindex"]);
    node.fund_wallet();

    let (_, tx) = node.create_mined_transaction();
    let txid = tx.compute_txid();

    let raw_tx = node.client.get_raw_transaction(txid).expect("getrawtransaction");
    let tx_out_proof = node.client.get_tx_out_proof(&[txid]).expect("gettxoutproof");

    let _: () =
        node.client.import_pruned_funds(&raw_tx.0, &tx_out_proof).expect("importprunedfunds");

    let _: () = node.client.remove_pruned_funds(txid).expect("removeprunedfunds");
}

#[test]
fn wallet__rescan_blockchain__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    let mining_addr = node.client.new_address().expect("newaddress");
    let _ = node.client.generate_to_address(3, &mining_addr).expect("generatetoaddress");

    let json: RescanBlockchain = node.client.rescan_blockchain().expect("rescanblockchain");
    let model: Result<mtype::RescanBlockchain, NumericError> = json.into_model();
    let rescan = model.unwrap();

    assert!(rescan.stop_height >= rescan.start_height);
}

// This is tested in `backup_and_restore_wallet()`, called by wallet__backup_wallet()
#[test]
#[cfg(not(feature = "v22_and_below"))]
fn wallet__restore_wallet() {}

// This is tested in raw_transactions.rs `create_sign_send()`.
#[test]
fn wallet__sign_raw_transaction_with_wallet__modelled() {}

#[test]
fn wallet__unload_wallet() { create_load_unload_wallet(); }

#[test]
fn wallet__send_many__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();

    let addr1 = node.client.new_address().expect("newaddress");
    let addr2 = node.client.new_address().expect("newaddress");

    let mut amounts = BTreeMap::new();
    amounts.insert(addr1, Amount::from_sat(100_000));
    amounts.insert(addr2, Amount::from_sat(100_000));

    let json: SendMany = node.client.send_many(amounts.clone()).expect("sendmany");
    let model: Result<mtype::SendMany, hex::HexToArrayError> = json.into_model();
    model.unwrap();

    #[cfg(not(feature = "v20_and_below"))]
    {
        let json_verbose: SendManyVerbose =
            node.client.send_many_verbose(amounts).expect("sendmany verbose");
        let model_verbose: Result<mtype::SendManyVerbose, hex::HexToArrayError> =
            json_verbose.into_model();
        model_verbose.unwrap();
    }
}

#[test]
#[cfg(not(feature = "v20_and_below"))]
fn wallet__send__modelled() {
    use std::collections::BTreeMap;

    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();
    let address = node.client.new_address().expect("failed to create new address");

    let mut outputs = BTreeMap::new();
    outputs.insert(address.to_string(), 0.001);

    let json: Send = node.client.send(&outputs).expect("send");
    let model: Result<mtype::Send, SendError> = json.into_model();
    model.unwrap();
}

#[test]
#[cfg(not(feature = "v23_and_below"))]
fn wallet__send_all__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();
    let address = node.client.new_address().expect("failed to create new address");

    let json: SendAll = node.client.send_all(&[address]).expect("sendall");
    let model: Result<mtype::SendAll, SendAllError> = json.into_model();
    model.unwrap();
}

#[test]
fn wallet__send_to_address__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();
    let address = node.client.new_address().expect("failed to create new address");

    let json: SendToAddress =
        node.client.send_to_address(&address, Amount::from_sat(10_000)).expect("sendtddress");
    let model: Result<mtype::SendToAddress, hex::HexToArrayError> = json.into_model();
    model.unwrap();
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__set_tx_fee() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    let fee_rate = FeeRate::from_sat_per_vb(2).expect("2 sat/vb is valid");

    let json: SetTxFee = node.client.set_tx_fee(fee_rate).expect("settxfee");
    assert!(json.0);
}

#[test]
#[cfg(not(feature = "v18_and_below"))]
fn wallet__set_wallet_flag() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    let json: SetWalletFlag = node.client.set_wallet_flag("avoid_reuse").expect("setwalletflag");
    assert_eq!(json.flag_name, "avoid_reuse");
    assert!(json.flag_state);
}

#[test]
#[cfg(feature = "v29_and_below")]
fn wallet__set_hd_seed() {
    let node = match () {
        #[cfg(feature = "v22_and_below")]
        () => Node::with_wallet(Wallet::Default, &[]),
        #[cfg(not(feature = "v22_and_below"))]
        () => {
            let node = Node::with_wallet(Wallet::None, &["-deprecatedrpc=create_bdb"]);
            node.client.create_legacy_wallet("wallet_name").expect("createlegacywallet");
            node
        }
    };

    node.fund_wallet();

    let _: () = node.client.set_hd_seed().expect("sethdseed");
}

#[test]
fn wallet__sign_message__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();

    let address = node.client.new_address_with_type(AddressType::Legacy).unwrap();
    let message = "integration test message";

    // Sign the message with the address key
    let json: SignMessage = node.client.sign_message(&address, message).expect("signmessage");
    let model: Result<mtype::SignMessage, sign_message::MessageSignatureError> = json.into_model();
    model.unwrap();
}

#[test]
#[cfg(not(feature = "v23_and_below"))]
fn wallet__simulate_raw_transaction() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();

    let address = node.client.new_address().expect("failed to create new address");
    let amount = Amount::from_sat(10_000);

    let txid1 =
        node.client.send_to_address(&address, amount).expect("sendtoaddress").txid().unwrap();
    let raw_tx1 = node.client.get_raw_transaction(txid1).expect("getrawtransaction");

    let txid2 =
        node.client.send_to_address(&address, amount).expect("sendtoaddress").txid().unwrap();
    let raw_tx2 = node.client.get_raw_transaction(txid2).expect("getrawtransaction");

    // Simulate raw transaction with the 2 transactions
    let rawtxs = vec![raw_tx1.0, raw_tx2.0];
    let json: SimulateRawTransaction =
        node.client.simulate_raw_transaction(&rawtxs).expect("simulaterawtransaction");

    let model: Result<mtype::SimulateRawTransaction, amount::ParseAmountError> = json.into_model();
    let raw_transaction = model.unwrap();

    // Should show a negative balance change since we're sending money
    assert!(raw_transaction.balance_change.is_negative());
}

#[test]
fn wallet__wallet_create_funded_psbt__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();

    let addr = node.client.new_address().expect("newaddress");
    let outputs = BTreeMap::from([(addr, Amount::from_sat(100_000))]);
    let json: WalletCreateFundedPsbt = node
        .client
        .wallet_create_funded_psbt(vec![], vec![outputs])
        .expect("walletcreatefundedpsbt");

    let model: Result<mtype::WalletCreateFundedPsbt, WalletCreateFundedPsbtError> =
        json.into_model();
    let psbt = model.unwrap();

    assert!(!psbt.psbt.inputs.is_empty());
}

#[test]
fn wallet__wallet_process_psbt__modelled() {
    let node = Node::with_wallet(Wallet::Default, &[]);
    node.fund_wallet();

    let addr = node.client.new_address().expect("newaddress");
    let outputs = BTreeMap::from([(addr, Amount::from_sat(50_000))]);
    let funded_psbt: WalletCreateFundedPsbt = node
        .client
        .wallet_create_funded_psbt(vec![], vec![outputs])
        .expect("walletcreatefundedpsbt");
    let model: Result<mtype::WalletCreateFundedPsbt, WalletCreateFundedPsbtError> =
        funded_psbt.into_model();
    let funded_psbt_model = model.unwrap();

    let json: WalletProcessPsbt =
        node.client.wallet_process_psbt(&funded_psbt_model.psbt).expect("walletprocesspsbt");
    #[cfg(feature = "v25_and_below")]
    type WalletProcessPsbtError = psbt::PsbtParseError;

    let model: Result<mtype::WalletProcessPsbt, WalletProcessPsbtError> = json.into_model();
    let processed = model.unwrap();

    assert_eq!(processed.psbt.inputs.len(), funded_psbt_model.psbt.inputs.len());
}

#[test]
fn wallet__wallet_lock() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    node.client.create_wallet("wallet_name").expect("createwallet");
    node.client.encrypt_wallet("passphrase").expect("encryptwallet");

    let _: () = node.client.wallet_lock().expect("walletlock");
}

#[test]
fn wallet__wallet_passphrase() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    node.client.create_wallet("wallet_name").expect("createwallet");
    node.client.encrypt_wallet("passphrase").expect("encryptwallet");

    let timeout = 60u64;
    let _: () = node.client.wallet_passphrase("passphrase", timeout).expect("walletpassphrase");
}

#[test]
fn wallet__wallet_passphrase_change() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    node.client.create_wallet("wallet name").expect("createwallet");
    node.client.encrypt_wallet("old passphrase").expect("encryptwallet");

    let _: () = node
        .client
        .wallet_passphrase_change("old passphrase", "new passphrase")
        .expect("walletpassphrasechange");
}

fn create_load_unload_wallet() {
    let node = Node::with_wallet(Wallet::None, &[]);

    let wallet = format!("wallet-{}", rand::random::<u32>()).to_string();
    node.client.create_wallet(&wallet).expect("failed to create wallet");

    // Upto version 20 Core returns null for `unloadwallet`.
    #[cfg(feature = "v20_and_below")]
    let _: () = node.client.unload_wallet(&wallet).expect("unloadwallet");

    // From version 21 Core returns warnings for `unloadwallet`.
    #[cfg(not(feature = "v20_and_below"))]
    {
        let json: UnloadWallet = node.client.unload_wallet(&wallet).expect("unloadwallet");
        let _: mtype::UnloadWallet = json.into_model();
    }

    let _: LoadWallet = node.client.load_wallet(&wallet).expect("loadwallet");
}

#[test]
#[cfg(all(feature = "v29_and_below", not(feature = "v20_and_below")))]
fn wallet__upgrade_wallet() {
    let node = Node::with_wallet(Wallet::Default, &[]);

    let _: UpgradeWallet = node.client.upgrade_wallet().expect("upgradewallet");
}
