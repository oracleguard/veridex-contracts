# Data Structures

## RiskScore
- wallet: Address
- asset_pair: Symbol
- score: u32 (0-100)
- confidence: u32 (0-100)
- timestamp: u64

## Market
- id: u64
- description: String
- token: Address
- close_time: u64
- outcome_count: u32
- status: MarketStatus

## Stake
- market_id: u64
- participant: Address
- outcome: u32
- amount: i128
