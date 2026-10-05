use rand::Rng;
use serde::Serialize;

/// 模拟风量/噪音测量仪的一次读数。
/// 真实部署时替换为与测量仪硬件通讯的实现即可，接口形状保持不变。
#[derive(Serialize, Clone)]
pub struct MeterReading {
    pub airflow_m3_min: f64,
    pub noise_db: f64,
    pub sampled_at_unix: u64,
}

pub fn take_reading() -> MeterReading {
    let mut rng = rand::thread_rng();
    let airflow = rng.gen_range(8.0..18.0_f64);
    let noise = rng.gen_range(45.0..70.0_f64);
    let sampled_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    MeterReading {
        airflow_m3_min: (airflow * 10.0).round() / 10.0,
        noise_db: (noise * 10.0).round() / 10.0,
        sampled_at_unix,
    }
}
