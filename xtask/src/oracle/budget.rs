use std::time::Duration;

pub const DIALOG_TIMEOUT_MS: u64 = 8_000;
pub const STEP_TIMEOUT_MS: u64 = 60_000;
pub const MAX_ATTEMPTS: u64 = 3;
pub const STEP_COUNT: u64 = 10;
pub const BACKSTOP_MARGIN_MS: u64 = 120_000;
pub const STARTUP_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepBudget {
    pub dialog_ms: u64,
    pub step_ms: u64,
    pub max_attempts: u64,
    pub steps: u64,
    pub margin_ms: u64,
    pub startup_ms: u64,
}

impl Default for StepBudget {
    fn default() -> Self {
        Self {
            dialog_ms: DIALOG_TIMEOUT_MS,
            step_ms: STEP_TIMEOUT_MS,
            max_attempts: MAX_ATTEMPTS,
            steps: STEP_COUNT,
            margin_ms: BACKSTOP_MARGIN_MS,
            startup_ms: STARTUP_TIMEOUT_MS,
        }
    }
}

impl StepBudget {
    pub fn per_step_ms(&self) -> u64 {
        self.max_attempts * self.dialog_ms + self.step_ms
    }

    pub fn sum_ms(&self) -> u64 {
        self.steps * self.per_step_ms()
    }

    pub fn backstop(&self) -> Duration {
        Duration::from_millis(self.sum_ms() + self.margin_ms)
    }

    pub fn startup(&self) -> Duration {
        Duration::from_millis(self.startup_ms)
    }

    pub fn script_args(&self) -> [(&'static str, String); 3] {
        [
            ("DialogTimeoutMs", self.dialog_ms.to_string()),
            ("StepTimeoutMs", self.step_ms.to_string()),
            ("MaxAttempts", self.max_attempts.to_string()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{DIALOG_TIMEOUT_MS, MAX_ATTEMPTS, STEP_TIMEOUT_MS, StepBudget};
    use std::path::Path;

    #[test]
    fn backstop_exceeds_the_sum_of_the_step_timeouts_by_the_margin() {
        let budget = StepBudget::default();
        assert_eq!(
            budget.per_step_ms(),
            MAX_ATTEMPTS * DIALOG_TIMEOUT_MS + STEP_TIMEOUT_MS
        );
        assert_eq!(budget.sum_ms(), budget.steps * budget.per_step_ms());
        let backstop_ms = u64::try_from(budget.backstop().as_millis()).expect("fits");
        assert_eq!(backstop_ms, budget.sum_ms() + budget.margin_ms);
        assert!(backstop_ms > budget.sum_ms());
        assert_eq!(
            StepBudget {
                startup_ms: 1,
                ..budget
            }
            .backstop(),
            budget.backstop()
        );
    }

    #[test]
    fn backstop_grows_with_every_component() {
        let base = StepBudget::default();
        for grown in [
            StepBudget {
                dialog_ms: base.dialog_ms + 1,
                ..base
            },
            StepBudget {
                step_ms: base.step_ms + 1,
                ..base
            },
            StepBudget {
                max_attempts: base.max_attempts + 1,
                ..base
            },
            StepBudget {
                steps: base.steps + 1,
                ..base
            },
            StepBudget {
                margin_ms: base.margin_ms + 1,
                ..base
            },
        ] {
            assert!(grown.backstop() > base.backstop());
        }
    }

    fn script_default(name: &str) -> u64 {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("oracle")
            .join("Invoke-GpExport.ps1");
        let text = std::fs::read_to_string(path).expect("script readable");
        let needle = format!("${name} = ");
        let start = text.find(&needle).expect("parameter declared") + needle.len();
        let digits: String = text[start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().expect("numeric default")
    }

    #[test]
    fn script_defaults_match_the_budget_constants() {
        let budget = StepBudget::default();
        assert_eq!(script_default("DialogTimeoutMs"), budget.dialog_ms);
        assert_eq!(script_default("StepTimeoutMs"), budget.step_ms);
        assert_eq!(script_default("MaxAttempts"), budget.max_attempts);
    }

    #[test]
    fn step_count_matches_the_steps_the_script_runs() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("oracle")
            .join("Invoke-GpExport.ps1");
        let text = std::fs::read_to_string(path).expect("script readable");
        assert_eq!(
            text.matches("Invoke-Step '").count() as u64,
            StepBudget::default().steps
        );
    }

    #[test]
    fn example_script_shares_the_budget_and_fits_its_backstop() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("oracle")
            .join("Invoke-GpExample.ps1");
        let text = std::fs::read_to_string(path).expect("script readable");
        let budget = StepBudget::default();
        for (name, value) in budget.script_args() {
            assert!(text.contains(&format!("[int]${name} = {value}")), "{name}");
        }
        let steps = text.matches("Invoke-Step '").count() as u64;
        assert!(steps >= 4);
        assert!(steps <= budget.steps);
    }

    #[test]
    fn script_args_carry_the_budget() {
        let args = StepBudget::default().script_args();
        assert_eq!(args[0], ("DialogTimeoutMs", String::from("8000")));
        assert_eq!(args[1], ("StepTimeoutMs", String::from("60000")));
        assert_eq!(args[2], ("MaxAttempts", String::from("3")));
    }
}
