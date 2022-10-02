#![allow(dead_code)]
use rand::Rng;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::thread;
use std::time::Duration;

mod math;

#[derive(PartialEq, Copy, Clone)]
enum Token {
    Eth,
    Dai,
}

fn price_to_tick(price: f64) -> f64 {
    price.log(1.001).floor()
}

fn price_to_sqrtp(price: f64) -> f64 {
    price.sqrt() * math::get_q96()
}

fn liquidity0(amount: f64, pa: f64, pb: f64) -> f64 {
    let q96 = math::get_q96();
    if pa > pb {
        return (amount * (pa * pb) / q96) / (pb - pa);
    } else {
        return (amount * (pb * pa) / q96) / (pa - pb);
    }
}

fn liquidity1(amount: f64, pa: f64, pb: f64) -> f64 {
    let q96 = math::get_q96();
    if pa > pb {
        return amount * q96 / (pb - pa);
    } else {
        return amount * q96 / (pa - pb);
    }
}

fn calc_amount0(liq: f64, lower_tick: f64, upper_tick: f64) -> f64 {
    let q96 = math::get_q96();

    liq * q96 * (upper_tick - lower_tick) / lower_tick / upper_tick
}

fn calc_amount1(liq: f64, lower_tick: f64, upper_tick: f64) -> f64 {
    liq * (upper_tick - lower_tick) / math::get_q96()
}

fn calc_price_diff(amount_in: f64, liquidity: f64) -> f64 {
    (amount_in * math::get_q96()) / liquidity
}

struct Tick {
    liquidity: RwLock<f64>,
    initialized: RwLock<bool>,
}

struct Position {
    liquidity: RwLock<f64>,
}

struct uniswap_v3_pool {
    token_0: Token,
    token_1: Token,
    min_tick: i32,
    max_tick: i32,
    balance_0: RwLock<f64>,
    balance_1: RwLock<f64>,
    tick_mapping: RwLock<HashMap<i32, Tick>>,
    liquidity_mapping: RwLock<HashMap<i32, f64>>,
    position_mapping: RwLock<HashMap<i32, Position>>,
    sqrt_price_x96: RwLock<f64>,
    tick: RwLock<i32>,
    liquidity: RwLock<f64>,
}

impl uniswap_v3_pool {
    fn update(&mut self, tick: i32, liquidity_delta: f64) -> bool {
        let default_tick = Tick {
            liquidity: RwLock::new(0.),
            initialized: RwLock::new(false),
        };
        let tick_map = &mut self.tick_mapping.write().unwrap();

        let info = tick_map.entry(tick).or_insert(default_tick);

        let liquidity_before = *info.liquidity.read().unwrap();

        let liquidity_after = liquidity_before + liquidity_delta;

        if liquidity_before == 0. {
            *info.initialized.write().unwrap() = true;
            self.liquidity_mapping
                .write()
                .unwrap()
                .insert(tick, liquidity_after);
        }

        *info.liquidity.write().unwrap() = liquidity_after;

        let flipped = (liquidity_after == 0.) != (liquidity_before == 0.);

        flipped
    }

    fn mint(&mut self, owner: &Trader, lower_tick: i32, upper_tick: i32, amount: f64) {
        if !(lower_tick >= upper_tick || lower_tick < self.min_tick || upper_tick > self.max_tick)
            && amount != 0.
        {
            let flipped_lower = self.update(lower_tick, amount);
            let flipped_upper = self.update(upper_tick, amount);

            if flipped_lower {
                self.liquidity_mapping
                    .write()
                    .unwrap()
                    .insert(lower_tick, 1.);
            }
            if flipped_upper {
                self.liquidity_mapping
                    .write()
                    .unwrap()
                    .insert(upper_tick, 1.);
            }

            let default_position = Position {
                liquidity: RwLock::new(0.),
            };

            let position_map = &mut self.position_mapping.write().unwrap();

            let position = position_map.entry(owner.id).or_insert(default_position);

            *position.liquidity.write().unwrap() += amount;

            let amount0 = calc_amount0(
                amount,
                *self.sqrt_price_x96.read().unwrap(),
                upper_tick.into(),
            );
            let amount1 = calc_amount1(
                amount,
                lower_tick.into(),
                *self.sqrt_price_x96.read().unwrap(),
            );

            if amount0 > 0. {
                *self.balance_0.write().unwrap() += amount0
            }
            if amount1 > 0. {
                *self.balance_1.write().unwrap() += amount1
            }
            *self.liquidity.write().unwrap() += amount
        }
    }


    fn next_initialized_tick(tick: i32, )
}
struct SwapState {
    amount_specified_remaining: f64,
    amount_calculated: f64,
    sqrt_price_x96: f64,
    tick: i32,
}

struct StepState {
    sqrt_price_start_x96: f64,
    next_tick: i32,
    sqrt_price_next_x96: f64,
    amount_in: f64,
    amount_out: f64,
}

fn v3_swap(
    trader: &mut Trader,
    pool: &uniswap_v3_pool,
    token_in: Token,
    amount_specified: f64,
    fee: f64,
) {
    let mut state = SwapState {
        amount_specified_remaining: amount_specified,
        amount_calculated: 0.,
        sqrt_price_x96: *pool.sqrt_price_x96.read().unwrap(),
        tick: *pool.tick.read().unwrap(),
    };


    while state.amount_specified_remaining > 0 {
        let step = StepState { sqrt_price_start_x96: state.sqrt_price_x96,
            next_tick: 
        
        
        }
    }



}

struct Trader {
    id: i32,
    amt_eth: RwLock<f64>,
    amt_dai: RwLock<f64>,
}

fn main() {}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn price_to_sqrt_price() {
        assert_eq!(price_to_sqrtp(5000.), 5.602277097478614e30);
    }

    #[test]
    fn v3_test_mint() {
        let trader = Trader {
            id: 2,
            amt_eth: RwLock::new(2000.),
            amt_dai: RwLock::new(10000.),
        };
        let mut pool = uniswap_v3_pool {
            liquidity: RwLock::new(0.),
            max_tick: math::get_max_tick(),
            min_tick: math::get_min_tick(),
            position_mapping: RwLock::new(HashMap::new()),
            tick_mapping: RwLock::new(HashMap::new()),
            liquidity_mapping: RwLock::new(HashMap::new()),
            sqrt_price_x96: RwLock::new(5602277097478614198912276234240.),
            tick: RwLock::new(85176),
            token_0: Token::Eth,
            token_1: Token::Dai,
            balance_0: RwLock::new(0.),
            balance_1: RwLock::new(0.),
        };

        pool.mint(&trader, 84222, 86129, 1517882343751509868544.);

        println!("{:?}", pool.balance_0);
        println!("{:?}", pool.balance_1);
        assert_eq!(*pool.liquidity.read().unwrap(), 1517882343751509868544.);
        assert_eq!(
            *pool.sqrt_price_x96.read().unwrap(),
            5602277097478614198912276234240.0
        );
    }
}
