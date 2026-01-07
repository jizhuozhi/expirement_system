use lazy_static::lazy_static;
use prometheus::{Counter, Histogram, IntCounter, Registry};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    
    // Request metrics
    pub static ref REQUEST_TOTAL: Counter = Counter::new(
        "experiment_requests_total",
        "Total number of experiment requests"
    ).unwrap();
    
    pub static ref REQUEST_ERRORS: Counter = Counter::new(
        "experiment_request_errors_total",
        "Total number of experiment request errors"
    ).unwrap();
    
    pub static ref REQUEST_DURATION: Histogram = Histogram::with_opts(
        prometheus::HistogramOpts::new(
            "experiment_request_duration_seconds",
            "Experiment request duration in seconds"
        )
        .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0])
    ).unwrap();
    
    // Layer metrics
    pub static ref LAYER_RELOAD_TOTAL: IntCounter = IntCounter::new(
        "experiment_layer_reload_total",
        "Total number of layer reloads"
    ).unwrap();
    
    pub static ref LAYER_RELOAD_ERRORS: IntCounter = IntCounter::new(
        "experiment_layer_reload_errors_total",
        "Total number of layer reload errors"
    ).unwrap();
    
    pub static ref ACTIVE_LAYERS: prometheus::IntGauge = prometheus::IntGauge::new(
        "experiment_active_layers",
        "Number of active layers"
    ).unwrap();
}

pub fn init() {
    REGISTRY.register(Box::new(REQUEST_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(REQUEST_ERRORS.clone())).unwrap();
    REGISTRY.register(Box::new(REQUEST_DURATION.clone())).unwrap();
    REGISTRY.register(Box::new(LAYER_RELOAD_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(LAYER_RELOAD_ERRORS.clone())).unwrap();
    REGISTRY.register(Box::new(ACTIVE_LAYERS.clone())).unwrap();
}
