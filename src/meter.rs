//! 模拟风量测量仪接口。
//!
//! 真实场景中读数来自师傅携带的测量仪表（蓝牙/串口网关），
//! 这里用确定性算法模拟一台仪表：清洗前读数偏低，清洗后明显上升，
//! 同一订单同一阶段重复读取结果稳定（模拟同一测点的多次读数）。

use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Before,
    After,
}

impl Phase {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "before" => Some(Phase::Before),
            "after" => Some(Phase::After),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Before => "before",
            Phase::After => "after",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MeterReading {
    pub airflow_m3h: f64,
    pub source: &'static str,
    pub note: String,
}

/// FNV-1a 64 位哈希，用来从订单号生成可复现的“仪表读数”
fn fnv1a64(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Default)]
pub struct MockInstrument;

impl MockInstrument {
    /// 读取一次风量（m³/h）。清洗前 620–799，清洗后 980–1179，
    /// 保证完工时“洗后风量 > 洗前风量”。
    pub fn read(&self, order_id: &str, phase: Phase) -> MeterReading {
        let h = fnv1a64(&format!("hoodcare:meter:{order_id}:{}", phase.as_str()));
        let (airflow_m3h, range) = match phase {
            Phase::Before => (620.0 + (h % 180) as f64, "620–800"),
            Phase::After => (980.0 + (h % 200) as f64, "980–1180"),
        };
        MeterReading {
            airflow_m3h: (airflow_m3h * 10.0).round() / 10.0,
            source: "mock-instrument-v1",
            note: format!("模拟测量仪读数（量程 {range} m³/h，标准工况测点）"),
        }
    }
}
