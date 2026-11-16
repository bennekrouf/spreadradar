//! What the wallet signs and sends, held to what was asked for.
//!
//! Jupiter builds the transaction, so its content is a remote server's word.
//! The limits in `trade.toml` used to be checked against the request only: a
//! reply holding a different transaction would have been signed and sent all
//! the same. Two checks close that, in dry runs as in live mode:
//!
//! * before signing, [`check_message`]: every instruction is one a swap is
//!   made of. A transfer of tokens, an approval or a call to an unknown
//!   program is refused unsigned.
//! * after simulating, [`check_after`]: what the node says the swap leaves in
//!   the wallet is no less than the size, the quote, the slippage limit and
//!   the fee cap allow, and nothing else was given a hold on it.
//!
//! The quote's price is still Jupiter's: these bound what a swap can take to
//! what that quote said, not the quote itself.

use std::str::FromStr;

use solana_keypair::Address;
use solana_transaction::VersionedMessage;

use super::rpc::Account;
use super::Side;
use crate::jupiter::{LAMPORTS_PER_SOL, USDT_MINT, USDT_UNITS};

const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ASSOCIATED_TOKEN_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const COMPUTE_BUDGET_PROGRAM: &str = "ComputeBudget111111111111111111111111111111";
const JUPITER_PROGRAM: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";

/// System program: `Transfer`, which wraps the SOL being sold.
const SYSTEM_TRANSFER: [u8; 4] = [2, 0, 0, 0];
/// Token program: `CloseAccount`, which unwraps SOL back into the wallet.
const TOKEN_CLOSE_ACCOUNT: u8 = 9;
/// Token program: `SyncNative`, which counts the SOL just wrapped.
const TOKEN_SYNC_NATIVE: u8 = 17;

/// Rent a new token account holds (the USDT account, on the first sale).
const TOKEN_ACCOUNT_RENT: u64 = 2_039_280;
/// The network's fee per signature.
const BASE_FEE: u64 = 5_000;

/// Refuses a transaction holding anything a SOL↔USDT swap is not made of.
///
/// Only the instructions Jupiter writes for one pass: the compute budget,
/// creating the wallet's token accounts, wrapping and unwrapping SOL, and the
/// swap itself. Programs are read from the transaction's own keys: one named
/// through a lookup table is refused, as the network does.
pub fn check_message(message: &VersionedMessage, wallet: &Address) -> Result<(), String> {
    let keys = message.static_account_keys();
    if keys.first() != Some(wallet) {
        return Err("its fees are not paid by this wallet".into());
    }
    for (n, ix) in message.instructions().iter().enumerate() {
        let program = keys
            .get(usize::from(ix.program_id_index))
            .map(Address::to_string)
            .ok_or_else(|| format!("instruction {} names no program", n + 1))?;
        let allowed = match program.as_str() {
            COMPUTE_BUDGET_PROGRAM | JUPITER_PROGRAM => true,
            // Create, or create unless it exists.
            ASSOCIATED_TOKEN_PROGRAM => matches!(ix.data.as_slice(), [] | [0] | [1]),
            SYSTEM_PROGRAM => ix.data.get(..4) == Some(&SYSTEM_TRANSFER[..]),
            TOKEN_PROGRAM => matches!(
                ix.data.first(),
                Some(&TOKEN_CLOSE_ACCOUNT | &TOKEN_SYNC_NATIVE)
            ),
            _ => false,
        };
        if !allowed {
            let kind = ix
                .data
                .first()
                .map_or_else(|| "no data".to_string(), |b| format!("kind {b}"));
            return Err(format!(
                "instruction {} is not part of a swap ({program}, {kind})",
                n + 1
            ));
        }
    }
    Ok(())
}

/// A token account's fields, read from its 165 bytes.
struct TokenAccount {
    mint: [u8; 32],
    owner: [u8; 32],
    amount: u64,
    /// The account that may spend from it, if one was given that right.
    delegate: Option<[u8; 32]>,
    /// The account that may close it, other than its owner.
    close_authority: Option<[u8; 32]>,
}

impl TokenAccount {
    const LEN: usize = 165;

    fn read(data: &[u8]) -> Option<Self> {
        if data.len() != Self::LEN {
            return None;
        }
        let key = |at: usize| -> Option<[u8; 32]> { data.get(at..at + 32)?.try_into().ok() };
        // An optional key: four bytes saying whether it is set, then the key.
        let optional = |at: usize| -> Option<Option<[u8; 32]>> {
            let tag: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
            Some(if u32::from_le_bytes(tag) == 0 {
                None
            } else {
                Some(key(at + 4)?)
            })
        };
        Some(Self {
            mint: key(0)?,
            owner: key(32)?,
            amount: u64::from_le_bytes(data.get(64..72)?.try_into().ok()?),
            delegate: optional(72)?,
            close_authority: optional(129)?,
        })
    }
}

fn usdt_mint() -> Result<[u8; 32], String> {
    Address::from_str(USDT_MINT)
        .map(|a| a.to_bytes())
        .map_err(|e| format!("the USDT mint is not an address: {e}"))
}

/// One of the wallet's USDT accounts, just before the swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsdtAccount {
    pub address: String,
    /// In USDT's smallest unit.
    pub amount: u64,
    /// Rights over it someone else already had: those are the owner's doing,
    /// and not the swap's.
    delegate: Option<[u8; 32]>,
    close_authority: Option<[u8; 32]>,
}

/// What the wallet holds just before the swap.
#[derive(Debug, Clone, PartialEq)]
pub struct Before {
    pub lamports: u64,
    pub usdt_accounts: Vec<UsdtAccount>,
}

impl Before {
    /// From the wallet's balance and its token accounts for USDT as the node gave them.
    pub fn new(lamports: u64, usdt_accounts: &[(String, Account)]) -> Self {
        Self {
            lamports,
            usdt_accounts: usdt_accounts
                .iter()
                .filter_map(|(address, account)| {
                    let token = TokenAccount::read(&account.data)?;
                    Some(UsdtAccount {
                        address: address.clone(),
                        amount: token.amount,
                        delegate: token.delegate,
                        close_authority: token.close_authority,
                    })
                })
                .collect(),
        }
    }

    fn usdt(&self) -> u64 {
        self.usdt_accounts
            .iter()
            .fold(0u64, |sum, account| sum.saturating_add(account.amount))
    }
}

/// The least the wallet may hold once the swap is through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Floor {
    pub lamports: u64,
    pub usdt: u64,
}

/// The floor for a swap of `size_sol` quoted at `quoted_usdt` in all.
///
/// Selling may take the size and the fees in SOL, and must bring the quoted
/// USDT less the slippage allowed. Buying may take the quoted USDT plus that
/// slippage, and must bring the size in SOL less the fees. Fees are the
/// priority fee cap, the network's own, and the rent of one new token account.
pub fn floor(
    before: &Before,
    side: Side,
    size_sol: f64,
    quoted_usdt: f64,
    slippage_bps: u16,
    max_priority_fee_lamports: u64,
) -> Result<Floor, String> {
    let size = (size_sol * LAMPORTS_PER_SOL).round() as u64;
    let fees = max_priority_fee_lamports.saturating_add(TOKEN_ACCOUNT_RENT + 2 * BASE_FEE);
    let quoted = (quoted_usdt * USDT_UNITS).round() as u64;
    let slip = (u128::from(quoted) * u128::from(slippage_bps) / 10_000) as u64;
    Ok(match side {
        Side::SellSol => Floor {
            // A wallet holding less than this loses, at most, all of it: still
            // no more than the size and the fees.
            lamports: before.lamports.saturating_sub(size.saturating_add(fees)),
            // One unit of leeway: Jupiter rounds its own threshold in integers.
            usdt: before
                .usdt()
                .saturating_add(quoted.saturating_sub(slip).saturating_sub(1)),
        },
        Side::BuySol => {
            let worst = quoted.saturating_add(slip).saturating_add(1);
            Floor {
                lamports: before.lamports.saturating_add(size).saturating_sub(fees),
                usdt: before.usdt().checked_sub(worst).ok_or_else(|| {
                    format!(
                        "not enough USDT: have {:.2}, need up to {:.2}",
                        before.usdt() as f64 / USDT_UNITS,
                        worst as f64 / USDT_UNITS
                    )
                })?,
            }
        }
    })
}

/// The accounts to ask the node about after the simulation: every one the
/// transaction names itself, and the wallet's USDT accounts.
///
/// A node answers for no more accounts than the transaction loads. A USDT
/// account it does not name itself can only be reached through a lookup
/// table, so there may be no more of those than the tables load.
pub fn watched(message: &VersionedMessage, before: &Before) -> Result<Vec<String>, String> {
    let mut addresses: Vec<String> = message
        .static_account_keys()
        .iter()
        .map(Address::to_string)
        .collect();
    let looked_up = message.address_table_lookups().map_or(0, |tables| {
        tables
            .iter()
            .map(|t| t.writable_indexes.len() + t.readonly_indexes.len())
            .sum()
    });
    let extra: Vec<String> = before
        .usdt_accounts
        .iter()
        .map(|account| account.address.clone())
        .filter(|address| !addresses.contains(address))
        .collect();
    if extra.len() > looked_up {
        return Err(
            "the wallet has more USDT accounts than this swap can be checked against".into(),
        );
    }
    addresses.extend(extra);
    Ok(addresses)
}

/// Refuses a swap that would leave the wallet with less than `floor`, or with
/// someone else holding a right over it that they did not have before.
///
/// `after` is the node's answer for each of `watched`, in the same order:
/// `None` for an account the transaction does not load, which it therefore
/// cannot change.
pub fn check_after(
    wallet: &Address,
    before: &Before,
    floor: &Floor,
    watched: &[String],
    after: &[Option<Account>],
) -> Result<(), String> {
    if watched.len() != after.len() {
        return Err("the node did not say what the swap leaves in the wallet".into());
    }
    let wallet_address = wallet.to_string();
    let wallet_key = wallet.to_bytes();
    let mint = usdt_mint()?;

    let sol = watched
        .iter()
        .zip(after)
        .find(|(address, _)| **address == wallet_address)
        .and_then(|(_, account)| account.as_ref())
        .ok_or("the node did not say what the swap leaves in the wallet")?;
    if sol.owner != SYSTEM_PROGRAM {
        return Err("it would hand the wallet over to another program".into());
    }
    if sol.lamports < floor.lamports {
        return Err(format!(
            "it would leave {:.6} SOL in the wallet, and this swap may not leave less than {:.6}",
            sol.lamports as f64 / LAMPORTS_PER_SOL,
            floor.lamports as f64 / LAMPORTS_PER_SOL
        ));
    }

    let mut usdt = 0u64;
    for (address, account) in watched.iter().zip(after) {
        let was = before.usdt_accounts.iter().find(|a| a.address == *address);
        let Some(account) = account else {
            // Not loaded, so not changed: it holds what it held.
            usdt = usdt.saturating_add(was.map_or(0, |a| a.amount));
            continue;
        };
        if account.owner != TOKEN_PROGRAM {
            continue;
        }
        let Some(token) = TokenAccount::read(&account.data) else {
            continue;
        };
        if token.owner != wallet_key {
            continue;
        }
        let new_delegate =
            token.delegate.is_some() && token.delegate != was.and_then(|a| a.delegate);
        let new_closer = token.close_authority.is_some()
            && token.close_authority != was.and_then(|a| a.close_authority);
        if new_delegate || new_closer {
            return Err(format!(
                "it would give another account a hold on the wallet's token account {address}"
            ));
        }
        if token.mint == mint {
            usdt = usdt.saturating_add(token.amount);
        }
    }
    // Every USDT account the wallet had was asked about, or it is not counted.
    if before
        .usdt_accounts
        .iter()
        .any(|account| !watched.contains(&account.address))
    {
        return Err("the node was not asked about every USDT account of the wallet".into());
    }
    if usdt < floor.usdt {
        return Err(format!(
            "it would leave {:.2} USDT in the wallet, and this swap may not leave less than {:.2}",
            usdt as f64 / USDT_UNITS,
            floor.usdt as f64 / USDT_UNITS
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_transaction::{CompiledInstruction, Message};

    const WALLET: [u8; 32] = [7; 32];
    const THIEF: [u8; 32] = [9; 32];

    fn address(bytes: [u8; 32]) -> Address {
        Address::new_from_array(bytes)
    }

    fn named(program: &str) -> Address {
        Address::from_str(program).unwrap()
    }

    /// A message paid by the wallet, with one instruction per `(program, data)`.
    fn message(instructions: &[(&str, &[u8])]) -> VersionedMessage {
        let mut keys = vec![address(WALLET)];
        let mut compiled = vec![];
        for (program, data) in instructions {
            keys.push(named(program));
            compiled.push(CompiledInstruction {
                program_id_index: (keys.len() - 1) as u8,
                accounts: vec![0],
                data: data.to_vec(),
            });
        }
        let mut message = Message::default();
        message.header.num_required_signatures = 1;
        message.account_keys = keys;
        message.instructions = compiled;
        VersionedMessage::Legacy(message)
    }

    fn token_account(mint: [u8; 32], owner: [u8; 32], amount: u64) -> Vec<u8> {
        let mut data = vec![0u8; TokenAccount::LEN];
        data[..32].copy_from_slice(&mint);
        data[32..64].copy_from_slice(&owner);
        data[64..72].copy_from_slice(&amount.to_le_bytes());
        data[108] = 1;
        data
    }

    fn usdt_account(owner: [u8; 32], amount: u64) -> Account {
        Account {
            lamports: TOKEN_ACCOUNT_RENT,
            owner: TOKEN_PROGRAM.into(),
            data: token_account(usdt_mint().unwrap(), owner, amount),
        }
    }

    fn wallet_account(lamports: u64) -> Account {
        Account {
            lamports,
            owner: SYSTEM_PROGRAM.into(),
            data: vec![],
        }
    }

    const SOL: u64 = 1_000_000_000;
    const USDT: u64 = 1_000_000;
    const ATA: &str = "usdt-account";

    fn held(address: &str, amount: u64) -> UsdtAccount {
        UsdtAccount {
            address: address.into(),
            amount,
            delegate: None,
            close_authority: None,
        }
    }

    /// 10 SOL and 500 USDT, selling 1 SOL quoted at 150 USDT, 0.5 % slippage.
    fn selling() -> (Before, Floor, Vec<String>) {
        let before = Before {
            lamports: 10 * SOL,
            usdt_accounts: vec![held(ATA, 500 * USDT)],
        };
        let floor = floor(&before, Side::SellSol, 1.0, 150.0, 50, 100_000).unwrap();
        let watched = vec![address(WALLET).to_string(), ATA.to_string()];
        (before, floor, watched)
    }

    fn after_sale(lamports: u64, usdt: Account) -> Vec<Option<Account>> {
        vec![Some(wallet_account(lamports)), Some(usdt)]
    }

    #[test]
    fn a_swap_as_jupiter_writes_it_is_signed() {
        let swap = message(&[
            (COMPUTE_BUDGET_PROGRAM, &[2, 0, 0, 0, 0]),
            (COMPUTE_BUDGET_PROGRAM, &[3, 0, 0, 0, 0, 0, 0, 0, 0]),
            (ASSOCIATED_TOKEN_PROGRAM, &[1]),
            (SYSTEM_PROGRAM, &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            (TOKEN_PROGRAM, &[TOKEN_SYNC_NATIVE]),
            (JUPITER_PROGRAM, &[0xe5, 0x17]),
            (TOKEN_PROGRAM, &[TOKEN_CLOSE_ACCOUNT]),
        ]);
        assert_eq!(check_message(&swap, &address(WALLET)), Ok(()));
    }

    #[test]
    fn anything_a_swap_is_not_made_of_is_refused_unsigned() {
        let wallet = address(WALLET);
        // Token transfer, approval, change of owner; handing the wallet to a program;
        // a program nobody knows.
        for (program, data) in [
            (TOKEN_PROGRAM, &[3u8, 1, 0, 0, 0, 0, 0, 0, 0][..]),
            (TOKEN_PROGRAM, &[4, 1, 0, 0, 0, 0, 0, 0, 0][..]),
            (TOKEN_PROGRAM, &[6, 0, 0][..]),
            (TOKEN_PROGRAM, &[12, 1, 0, 0, 0, 0, 0, 0, 0, 6][..]),
            (SYSTEM_PROGRAM, &[1, 0, 0, 0][..]),
            (SYSTEM_PROGRAM, &[][..]),
            (ASSOCIATED_TOKEN_PROGRAM, &[2][..]),
            ("Stake11111111111111111111111111111111111111", &[0][..]),
        ] {
            let bad = message(&[(JUPITER_PROGRAM, &[0xe5]), (program, data)]);
            let refused = check_message(&bad, &wallet).unwrap_err();
            assert!(refused.contains("instruction 2"), "{program}: {refused}");
        }
        // Paid by someone else: not this wallet's swap.
        let swap = message(&[(JUPITER_PROGRAM, &[0xe5])]);
        assert!(check_message(&swap, &address(THIEF)).is_err());
    }

    #[test]
    fn a_program_past_the_transactions_own_keys_is_refused() {
        let mut swap = message(&[(JUPITER_PROGRAM, &[0xe5])]);
        if let VersionedMessage::Legacy(m) = &mut swap {
            m.instructions[0].program_id_index = 40;
        }
        assert!(check_message(&swap, &address(WALLET)).is_err());
    }

    #[test]
    fn the_floor_follows_the_size_the_quote_and_the_limits() {
        let (before, sell, _) = selling();
        let fees = 100_000 + TOKEN_ACCOUNT_RENT + 2 * BASE_FEE;
        assert_eq!(sell.lamports, 9 * SOL - fees);
        // 150 USDT less 0.5 %, less the unit of leeway.
        assert_eq!(sell.usdt, 500 * USDT + 149_250_000 - 1);

        let buy = floor(&before, Side::BuySol, 1.0, 150.0, 50, 100_000).unwrap();
        assert_eq!(buy.lamports, 11 * SOL - fees);
        assert_eq!(buy.usdt, 500 * USDT - 150_750_000 - 1);

        // A quote for more USDT than the wallet holds is no swap at all.
        let refused = floor(&before, Side::BuySol, 1.0, 500.0, 50, 100_000).unwrap_err();
        assert!(refused.contains("not enough USDT"), "{refused}");
    }

    #[test]
    fn a_swap_within_its_limits_goes_through() {
        let (before, floor, watched) = selling();
        let after = after_sale(9 * SOL - 20_000, usdt_account(WALLET, 649_900_000));
        assert_eq!(
            check_after(&address(WALLET), &before, &floor, &watched, &after),
            Ok(())
        );
    }

    #[test]
    fn a_swap_that_takes_more_than_its_size_is_not_sent() {
        let (before, floor, watched) = selling();
        let wallet = address(WALLET);
        // All the SOL gone, for the USDT of one.
        let drained = after_sale(1_000, usdt_account(WALLET, 650 * USDT));
        let why = check_after(&wallet, &before, &floor, &watched, &drained).unwrap_err();
        assert!(why.contains("SOL"), "{why}");
        // The SOL sold, and nothing for it.
        let unpaid = after_sale(9 * SOL - 20_000, usdt_account(WALLET, 500 * USDT));
        let why = check_after(&wallet, &before, &floor, &watched, &unpaid).unwrap_err();
        assert!(why.contains("USDT"), "{why}");
        // The USDT account emptied on the way.
        let emptied = after_sale(9 * SOL - 20_000, usdt_account(WALLET, 0));
        assert!(check_after(&wallet, &before, &floor, &watched, &emptied).is_err());
    }

    #[test]
    fn a_hold_given_to_someone_else_is_not_sent() {
        let (before, floor, watched) = selling();
        let wallet = address(WALLET);
        let paid = 650 * USDT;

        // The USDT account now belongs to another key: it no longer counts.
        let taken = after_sale(9 * SOL - 20_000, usdt_account(THIEF, paid));
        assert!(check_after(&wallet, &before, &floor, &watched, &taken).is_err());

        // Still the wallet's, but another account may spend from it, or close it.
        for tag_at in [72, 129] {
            let mut account = usdt_account(WALLET, paid);
            account.data[tag_at] = 1;
            account.data[tag_at + 4..tag_at + 36].copy_from_slice(&THIEF);
            let after = after_sale(9 * SOL - 20_000, account);
            let why = check_after(&wallet, &before, &floor, &watched, &after).unwrap_err();
            assert!(why.contains("hold"), "{why}");

            // A right that key already had is the owner's doing: the swap goes through.
            let mut already = before.clone();
            if tag_at == 72 {
                already.usdt_accounts[0].delegate = Some(THIEF);
            } else {
                already.usdt_accounts[0].close_authority = Some(THIEF);
            }
            assert_eq!(
                check_after(&wallet, &already, &floor, &watched, &after),
                Ok(())
            );
            // Unless it went to a different key.
            already.usdt_accounts[0].delegate = Some(WALLET);
            already.usdt_accounts[0].close_authority = Some(WALLET);
            assert!(check_after(&wallet, &already, &floor, &watched, &after).is_err());
        }

        // The wallet itself handed to a program.
        let mut handed = after_sale(9 * SOL - 20_000, usdt_account(WALLET, paid));
        if let Some(Some(sol)) = handed.first_mut() {
            sol.owner = TOKEN_PROGRAM.into();
        }
        let why = check_after(&wallet, &before, &floor, &watched, &handed).unwrap_err();
        assert!(why.contains("another program"), "{why}");
    }

    #[test]
    fn an_account_the_swap_does_not_load_holds_what_it_held() {
        // A second USDT account, with 100 USDT, that the swap never touches.
        let (mut before, _, mut watched) = selling();
        before.usdt_accounts.push(held("spare", 100 * USDT));
        watched.push("spare".into());
        let floor = floor(&before, Side::SellSol, 1.0, 150.0, 50, 100_000).unwrap();
        let mut after = after_sale(9 * SOL - 20_000, usdt_account(WALLET, 649_900_000));
        after.push(None);
        let wallet = address(WALLET);
        assert_eq!(
            check_after(&wallet, &before, &floor, &watched, &after),
            Ok(())
        );
        // Loaded and emptied, it is missed.
        after[2] = Some(usdt_account(WALLET, 0));
        assert!(check_after(&wallet, &before, &floor, &watched, &after).is_err());
    }

    #[test]
    fn an_answer_that_leaves_the_wallet_out_is_not_trusted() {
        let (before, floor, watched) = selling();
        let wallet = address(WALLET);
        let paid = usdt_account(WALLET, 650 * USDT);
        assert!(check_after(&wallet, &before, &floor, &watched, &[]).is_err());
        let silent = vec![None, Some(paid)];
        assert!(check_after(&wallet, &before, &floor, &watched, &silent).is_err());
    }

    #[test]
    fn every_usdt_account_of_the_wallet_is_asked_about() {
        let swap = message(&[(JUPITER_PROGRAM, &[0xe5])]);
        let before = Before {
            lamports: SOL,
            usdt_accounts: vec![held(&address(WALLET).to_string(), 1)],
        };
        // One the transaction names itself is not asked about twice.
        let asked = watched(&swap, &before).unwrap();
        assert_eq!(asked.len(), 2);
        // One it does not name, with no lookup table to reach it through.
        let before = Before {
            lamports: SOL,
            usdt_accounts: vec![held("elsewhere", 1)],
        };
        assert!(watched(&swap, &before).is_err());
    }
}
