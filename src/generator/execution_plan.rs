use super::{Generator, MockGenerator, NamedTxRequest};
use crate::Result;

pub trait ExecutionPlan<G: Generator> {
    fn new(gen: G) -> Self;
    fn send_txs(&self) -> Result<()>;
}

pub struct MockExecutionPlan {
    generator: MockGenerator,
    txs: Vec<NamedTxRequest>,
}

impl MockExecutionPlan {
    pub fn init(&mut self, amount: usize) -> Result<()> {
        let txs = self.generator.get_txs(amount)?;
        println!("Generated {} txs", txs.len());
        self.txs = txs;
        Ok(())
    }
}

impl ExecutionPlan<MockGenerator> for MockExecutionPlan {
    fn new(generator: MockGenerator) -> Self {
        MockExecutionPlan {
            generator,
            txs: vec![],
        }
    }

    fn send_txs(&self) -> Result<()> {
        if self.txs.len() == 0 {
            return Err(crate::error::ContenderError::SpamError(
                "No txs to send",
                None,
            ));
        }
        println!("Sending {} txs", self.txs.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::MockGenerator;

    #[test]
    fn test_mock_execution_plan() {
        let gen = MockGenerator;
        let mut plan = MockExecutionPlan::new(gen);
        let init_res = plan.init(10);
        assert!(init_res.is_ok());
        let send_res = plan.send_txs();
        assert!(send_res.is_ok());
    }
}
