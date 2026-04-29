// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Manual benchmark for [`BlockLoader::load_many`].
//!
//! Run with:
//! ```sh
//! cargo nextest run -p ethexe-observer --no-capture \
//!     --run-ignored only -- bench::bench_load_many
//! ```

use crate::utils::{BlockLoader, EthereumBlockLoader};
use alloy::{
    node_bindings::Anvil,
    providers::{Provider, ProviderBuilder, ext::AnvilApi},
};
use anyhow::Result;
use ethexe_common::Address;
use ethexe_ethereum::deploy::EthereumDeployer;
use gsigner::secp256k1::Signer;
use std::time::Instant;

const BENCH_BLOCKS: u64 = 1500;
const BENCH_REPEATS: usize = 5;

#[tokio::test]
#[ignore = "manual benchmark, requires anvil"]
async fn bench_load_many() -> Result<()> {
    gear_utils::init_default_logger();

    let anvil = Anvil::new().try_spawn()?;
    let ethereum_rpc = anvil.ws_endpoint();

    let signer = Signer::memory();
    let sender_public_key = signer
        .import("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?)?;
    let sender_address = sender_public_key.to_address();
    let validators: Vec<Address> = vec!["0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1".parse()?];

    let deployer = EthereumDeployer::new(&ethereum_rpc, signer, sender_address)
        .await
        .unwrap();
    let ethereum = deployer
        .with_validators(validators.try_into().unwrap())
        .deploy()
        .await?;

    let provider = ProviderBuilder::default().connect(&ethereum_rpc).await?;

    // Sprinkle some events along the chain so the log filter has work to do.
    let wat_template =
        "(module (export \"init\" (func $init)) (func $init (drop (i32.const {N}))))";
    for n in 0..16u32 {
        let wat = wat_template.replace("{N}", &n.to_string());
        let wasm = wat::parse_str(&wat)?;
        ethereum.router().request_code_validation(&wasm).await?;
    }

    // Mine empty blocks up to the target height to grow the range.
    let head = provider.get_block_number().await?;
    if head < BENCH_BLOCKS {
        provider.anvil_mine(Some(BENCH_BLOCKS - head), None).await?;
    }
    let head_number = provider.get_block_number().await?;
    println!("[bench] chain head = {head_number}");

    let loader = EthereumBlockLoader::new(provider.clone(), ethereum.router().address());

    let mut samples = Vec::with_capacity(BENCH_REPEATS);
    for i in 0..BENCH_REPEATS {
        let start = Instant::now();
        let blocks = loader.load_many(0..=head_number).await?;
        let elapsed = start.elapsed();
        println!(
            "[bench] iter {i}: load_many(0..={head_number}) = {} blocks in {:?}",
            blocks.len(),
            elapsed
        );
        samples.push(elapsed);
    }

    let total: std::time::Duration = samples.iter().sum();
    let avg = total / samples.len() as u32;
    let min = *samples.iter().min().unwrap();
    let max = *samples.iter().max().unwrap();
    println!("[bench] head={head_number} avg={avg:?} min={min:?} max={max:?} runs={BENCH_REPEATS}");

    Ok(())
}
