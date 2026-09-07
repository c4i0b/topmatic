use std::net::TcpStream;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub delay_factor: u32,
    pub budget: Duration,
    pub network_cap: Duration,
    pub poll: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(2 * 60),
            delay_factor: 3,
            budget: Duration::from_secs(45 * 60),
            network_cap: Duration::from_secs(10 * 60),
            poll: Duration::from_secs(10),
        }
    }
}

impl RetryPolicy {
    pub fn none() -> Self {
        Self {
            max_retries: 0,
            base_delay: Duration::ZERO,
            delay_factor: 1,
            budget: Duration::ZERO,
            network_cap: Duration::ZERO,
            poll: Duration::ZERO,
        }
    }

    pub fn delay_for(&self, retry: u32) -> Duration {
        self.base_delay * self.delay_factor.pow(retry - 1)
    }
}

pub fn run_with_retries(
    policy: &RetryPolicy,
    mut now: impl FnMut() -> Duration,
    mut gate: impl FnMut(Duration) -> bool,
    mut attempt: impl FnMut() -> bool,
) -> bool {
    if attempt() {
        return true;
    }
    for retry in 1..=policy.max_retries {
        let delay = policy.delay_for(retry);
        if now() + delay > policy.budget {
            break;
        }
        gate(delay);
        if attempt() {
            return true;
        }
    }
    false
}

const PROBE_TARGETS: [(&str, u16); 2] = [("1.1.1.1", 443), ("9.9.9.9", 443)];

pub fn tcp_online() -> bool {
    for (host, port) in PROBE_TARGETS {
        let Ok(address) = format!("{host}:{port}").parse() else {
            continue;
        };
        if TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok() {
            return true;
        }
    }
    false
}

pub fn wait_online_or_cap(probe: impl Fn() -> bool, cap: Duration, poll: Duration) -> bool {
    let start = Instant::now();
    loop {
        if probe() {
            return true;
        }
        if start.elapsed() >= cap {
            return false;
        }
        std::thread::sleep(poll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn policy() -> RetryPolicy {
        RetryPolicy::default()
    }

    #[test]
    fn delays_grow_incrementally() {
        let policy = policy();
        assert_eq!(policy.delay_for(1), Duration::from_secs(2 * 60));
        assert_eq!(policy.delay_for(2), Duration::from_secs(6 * 60));
        assert_eq!(policy.delay_for(3), Duration::from_secs(18 * 60));
    }

    struct Harness {
        clock: RefCell<Duration>,
        gates: RefCell<Vec<Duration>>,
        attempts: RefCell<u32>,
        results: Vec<bool>,
    }

    impl Harness {
        fn new(results: &[bool]) -> Self {
            Self {
                clock: RefCell::new(Duration::ZERO),
                gates: RefCell::new(Vec::new()),
                attempts: RefCell::new(0),
                results: results.to_vec(),
            }
        }

        fn run(&self, policy: &RetryPolicy) -> bool {
            run_with_retries(
                policy,
                || *self.clock.borrow(),
                |delay| {
                    self.gates.borrow_mut().push(delay);
                    *self.clock.borrow_mut() += delay;
                    true
                },
                || {
                    let attempt = *self.attempts.borrow();
                    *self.attempts.borrow_mut() += 1;
                    self.results[(attempt as usize).min(self.results.len() - 1)]
                },
            )
        }
    }

    #[test]
    fn first_attempt_success_never_gates() {
        let harness = Harness::new(&[true]);
        assert!(harness.run(&policy()));
        assert_eq!(*harness.attempts.borrow(), 1);
        assert!(harness.gates.borrow().is_empty());
    }

    #[test]
    fn exhausted_retries_gate_with_incremental_delays() {
        let harness = Harness::new(&[false]);
        assert!(!harness.run(&policy()));
        assert_eq!(*harness.attempts.borrow(), 4, "initial + 3 retries");
        assert_eq!(
            *harness.gates.borrow(),
            vec![
                Duration::from_secs(2 * 60),
                Duration::from_secs(6 * 60),
                Duration::from_secs(18 * 60),
            ]
        );
    }

    #[test]
    fn recovery_stops_the_loop() {
        let harness = Harness::new(&[false, false, true]);
        assert!(harness.run(&policy()));
        assert_eq!(*harness.attempts.borrow(), 3);
        assert_eq!(harness.gates.borrow().len(), 2);
    }

    #[test]
    fn budget_guard_skips_retries_that_would_overflow() {
        let harness = Harness::new(&[false]);
        *harness.clock.borrow_mut() = Duration::from_secs(44 * 60);
        assert!(!harness.run(&policy()));
        assert_eq!(
            *harness.attempts.borrow(),
            1,
            "a 2min delay on top of 44min elapsed would break the 45min budget"
        );
        assert!(harness.gates.borrow().is_empty());
    }

    #[test]
    fn no_retry_policy_runs_exactly_once() {
        let harness = Harness::new(&[false]);
        assert!(!harness.run(&RetryPolicy::none()));
        assert_eq!(*harness.attempts.borrow(), 1);
    }

    #[test]
    fn wait_online_returns_as_soon_as_the_probe_agrees() {
        let calls = std::cell::Cell::new(0);
        let started = Instant::now();
        assert!(wait_online_or_cap(
            || {
                calls.set(calls.get() + 1);
                calls.get() >= 2
            },
            Duration::from_secs(30),
            Duration::from_millis(1)
        ));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn wait_online_gives_up_at_the_cap() {
        let started = Instant::now();
        assert!(!wait_online_or_cap(
            || false,
            Duration::from_millis(5),
            Duration::from_millis(1)
        ));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
