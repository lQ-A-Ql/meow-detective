//! Regularized upper incomplete gamma function, Numerical Recipes style.

const MAX_ITERATIONS: usize = 1000;
const EPSILON: f64 = 1e-15;
const TINY: f64 = 1e-300;

pub(crate) fn regularized_gamma_q(a: f64, x: f64) -> f64 {
    if !a.is_finite() || a <= 0.0 || !x.is_finite() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        1.0 - gamma_p_series(a, x)
    } else {
        gamma_q_continued_fraction(a, x)
    }
}

fn gamma_p_series(a: f64, x: f64) -> f64 {
    let mut ap = a;
    let mut sum = 1.0 / a;
    let mut delta = sum;
    for _ in 0..MAX_ITERATIONS {
        ap += 1.0;
        delta *= x / ap;
        sum += delta;
        if delta.abs() < sum.abs() * EPSILON {
            break;
        }
    }
    sum * prefactor(a, x)
}

fn gamma_q_continued_fraction(a: f64, x: f64) -> f64 {
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / TINY;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..MAX_ITERATIONS {
        let index = i as f64;
        let an = -index * (index - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < TINY {
            d = TINY;
        }
        c = b + an / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < EPSILON {
            break;
        }
    }
    prefactor(a, x) * h
}

fn prefactor(a: f64, x: f64) -> f64 {
    (-x + a * x.ln() - ln_gamma(a)).exp()
}

fn ln_gamma(x: f64) -> f64 {
    const COEFFICIENTS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    const G: f64 = 7.0;
    if x < 0.5 {
        return std::f64::consts::PI.ln()
            - (std::f64::consts::PI * x).sin().ln()
            - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut sum = COEFFICIENTS[0];
    let t = x + G + 0.5;
    for (index, coefficient) in COEFFICIENTS.iter().enumerate().skip(1) {
        sum += coefficient / (x + index as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}
