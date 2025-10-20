use malachitebft_core_types::CommitCertificate;

use crate::{context::TestContext, value::Value};

#[derive(Clone, Debug)]
pub struct DecidedValue {
    pub value: Value,
    pub certificate: CommitCertificate<TestContext>,
}
