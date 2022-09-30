#![allow(dead_code)]
#![allow(unused_imports)]
use rand::Rng;
use std::cell::Cell;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(PartialEq)]
enum Token {
    Eth,
    Dai,
}

struct Pool {
    token_x: Token,
    token_y: Token,
    x: Cell<f64>,
    y: Cell<f64>,
    k: Cell<f64>,
}

struct Trader {
    amt_eth: Cell<f64>,
    amt_dai: Cell<f64>,
}

fn add(pool: &Pool, add_to_x: f64, add_to_y: f64) {
    pool.x.set(pool.x.get() + add_to_x);
    pool.y.set(pool.y.get() + add_to_y);
    pool.k.set(pool.y.get() + pool.x.get());
}

fn remove(pool: &Pool, rem_from_x: f64, rem_from_y: f64) {
    pool.x.set(pool.x.get() - rem_from_x);
    pool.y.set(pool.y.get() - rem_from_y);
    pool.k.set(pool.y.get() + pool.x.get());
}

fn get_amount_out(amount_in: f64, pool: &Pool, token_in: Token, fee: f64) -> f64 {
    let amount_in_less_fee = amount_in * (1. - fee);
    let py = pool.y.get();
    let px = pool.x.get();
    if token_in == pool.token_x {
        let price = py / px;
        let amount_out = amount_in_less_fee * price;
        if amount_out <= py {
            remove(pool, 0., amount_out);
            add(pool, amount_in, 0.);
            return amount_out;
        } else {
            return 0.;
        }
    } else {
        let price = px / py;
        let amount_out = amount_in_less_fee * price;
        if amount_out <= px {
            remove(pool, amount_out, 0.);
            add(pool, 0., amount_in);
            return amount_out;
        } else {
            return 0.;
        }
    }
}

fn swap(trader: &Trader, pool: &Pool, token_in: Token, amount_in: f64, fee: f64) {
    let amt_eth = trader.amt_eth.get();
    let amt_dai = trader.amt_dai.get();
    if token_in == Token::Eth && amt_eth > amount_in {
        let amt_out = get_amount_out(amount_in, pool, token_in, fee);
        if amt_out > 0. {
            trader.amt_eth.set(amt_eth - amount_in);
            trader.amt_dai.set(amt_dai + amt_out);
        }
    } else if token_in == Token::Dai && amt_dai > amount_in {
        let amt_out = get_amount_out(amount_in, pool, token_in, fee);
        if amt_out > 0. {
            trader.amt_eth.set(amt_eth + amt_out);
            trader.amt_dai.set(amt_dai - amount_in);
        }
    }
}

fn calc_two_pool_arb_profit(x_in: f64, xr1: f64, xr2: f64, yr1: f64, yr2: f64, fee: f64) -> f64 {
    let s = (fee * xr1 * x_in) / (yr1 + (fee * x_in));
    let n = fee * yr2 * s;
    let d = xr2 + fee * s;
    n / d
}

// fn calc_two_pool_arb_optimal_amount(
//     xr1: f64,
//     xr2: f64,
//     yr1: f64,
//     yr2: f64,
//     fee: f64,
// ) -> (f64, f64) {
//     let n1 = (xr1 * xr2 * yr1 * yr2).sqrt() * fee;
//     let n2 = yr1 * yr2;
//     let d = fee * (xr2 + (fee * xr1));
//     ((n1 - n2) / d, ((-n1) - n2) / d)
// }

fn detect_arb(pool1: &Pool, pool2: &Pool, token_in: Token, fee: f64) -> f64 {
    let is_x_1 = token_in == pool1.token_x;
    let is_x_2 = token_in == pool2.token_x;

    let x1 = pool1.x.get();
    let x2 = pool2.x.get();
    let y1 = pool1.y.get();
    let y2 = pool2.y.get();

    let amt_in = 1.;

    let opt_amt = match (is_x_1, is_x_2) {
        (true, true) => calc_two_pool_arb_profit(amt_in, x1, x2, y1, y2, fee),
        (true, false) => calc_two_pool_arb_profit(amt_in, x1, y2, y1, x2, fee),
        (false, false) => calc_two_pool_arb_profit(amt_in, y1, y2, x1, x2, fee),
        (false, true) => calc_two_pool_arb_profit(amt_in, y1, x2, x1, y2, fee),
    };

    opt_amt
}

fn main() {
    let xx = 1000.;
    let yy = 200.;
    let kk = xx * yy;

    let pool = Pool {
        token_x: Token::Eth,
        token_y: Token::Dai,
        x: Cell::new(xx),
        y: Cell::new(yy),
        k: Cell::new(kk),
    };

    let trader = Trader {
        amt_eth: Cell::new(xx),
        amt_dai: Cell::new(yy),
    };

    pool.x.replace(5.);

    add(&pool, 4., 4.);

    println!("{:?}", trader.amt_eth.get());
    println!("{:?}", trader.amt_dai.get());

    swap(&trader, &pool, Token::Eth, 2., 0.03);

    println!("{:?}", trader.amt_eth.get());
    println!("{:?}", trader.amt_dai.get());

    let pool1 = Pool {
        token_x: Token::Eth,
        token_y: Token::Dai,
        x: Cell::new(4.),
        y: Cell::new(3500.),
        k: Cell::new(4000. + 4.),
    };

    let pool2 = Pool {
        token_x: Token::Eth,
        token_y: Token::Dai,
        x: Cell::new(4.),
        y: Cell::new(4000.),
        k: Cell::new(3500. + 4.),
    };

    let b1 = detect_arb(&pool2, &pool1, Token::Eth, 0.97);
    let b2 = detect_arb(&pool1, &pool2, Token::Eth, 0.97);

    // let handle = thread::spawn(|| {
    //     for i in 1..100 {
    //         let mut rng = rand::thread_rng();
    //         let randomness = rng.gen_range(0..10);

    //         if randomness > 5 {
    //             add(&pool1, 1., 1000.);
    //             add(&pool2, 1., 1200.);
    //         } else {
    //             remove(&pool1, 1., 1000.);
    //             remove(&pool2, 1., 1200.);
    //         }
    //     }
    // });
}
