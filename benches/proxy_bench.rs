use claude_code_proxy::models::claude::*;
use claude_code_proxy::streaming::{SSEEventGenerator, StreamingJsonParser};
use claude_code_proxy::transform::*;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

fn benchmark_model_mapping(c: &mut Criterion) {
    let models = vec![
        "claude-3-5-sonnet-20241022",
        "claude-3-opus-20240229",
        "claude-3-haiku-20240307",
        "unknown-model",
    ];

    c.bench_function("model_name_mapping", |b| {
        b.iter(|| {
            for model in &models {
                black_box(map_model_name(model));
            }
        });
    });
}

fn benchmark_request_validation(c: &mut Criterion) {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text("Hello".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: ContentType::Text("Hi there!".to_string()),
            },
        ],
        system: Some(SystemPrompt::Text(
            "You are a helpful assistant".to_string(),
        )),
        max_tokens: Some(100),
        temperature: Some(0.7),
        stop_sequences: None,
        stream: true,
        top_p: Some(0.9),
        top_k: Some(40),
        tools: None,
    };

    c.bench_function("validate_claude_request", |b| {
        b.iter(|| {
            black_box(validate_claude_request(&req)).unwrap();
        });
    });
}

fn benchmark_request_transformation(c: &mut Criterion) {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text("What is Rust?".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: ContentType::Text("Rust is a systems programming language.".to_string()),
            },
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text("Tell me more.".to_string()),
            },
        ],
        system: Some(SystemPrompt::Text("You are a Rust expert.".to_string())),
        max_tokens: Some(500),
        temperature: Some(0.7),
        stop_sequences: Some(vec!["STOP".to_string()]),
        stream: true,
        top_p: Some(0.9),
        top_k: Some(40),
        tools: None,
    };

    c.bench_function("transform_request", |b| {
        b.iter(|| {
            black_box(transform_request(req.clone()).unwrap());
        });
    });
}

fn benchmark_json_serialization(c: &mut Criterion) {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Benchmark test".to_string()),
        }],
        system: None,
        max_tokens: Some(100),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: None,
    };

    let gemini_req = transform_request(req).unwrap();

    c.bench_function("serialize_gemini_request", |b| {
        b.iter(|| {
            black_box(serde_json::to_vec(&gemini_req).unwrap());
        });
    });
}

fn benchmark_streaming_parser(c: &mut Criterion) {
    let data = br#"[
        {"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"}}],"usageMetadata":{"promptTokenCount":10}},
        {"candidates":[{"content":{"parts":[{"text":" world"}],"role":"model"}}]},
        {"candidates":[{"content":{"parts":[{"text":"!"}],"role":"model"},"finishReason":"STOP"}]}
    ]"#;

    let mut group = c.benchmark_group("streaming_parser");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("parse_complete_stream", |b| {
        b.iter(|| {
            let mut parser = StreamingJsonParser::new();
            black_box(parser.feed(data).unwrap());
        });
    });

    group.finish();
}

fn benchmark_streaming_parser_incremental(c: &mut Criterion) {
    let chunk1 = br#"[{"candidates":[{"content":{"parts":[{"text":"H"#;
    let chunk2 = br#"ello"}],"role":"model"}}]},"#;
    let chunk3 = br#"{"candidates":[{"content":{"parts":[{"text":" world"}],"role":"model"}}]}]"#;

    c.bench_function("parse_incremental_stream", |b| {
        b.iter(|| {
            let mut parser = StreamingJsonParser::new();
            parser.feed(chunk1).unwrap();
            parser.feed(chunk2).unwrap();
            black_box(parser.feed(chunk3).unwrap());
        });
    });
}

fn benchmark_sse_generation(c: &mut Criterion) {
    use claude_code_proxy::models::gemini::*;

    let chunks = vec![
        GeminiStreamChunk {
            candidates: vec![Candidate {
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![GeminiPart::Text {
                        text: "Hello".to_string(),
                    }],
                }),
                finish_reason: None,
                safety_ratings: None,
                index: None,
            }],
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: Some(10),
                candidates_token_count: None,
                total_token_count: None,
            }),
            prompt_feedback: None,
        },
        GeminiStreamChunk {
            candidates: vec![Candidate {
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![GeminiPart::Text {
                        text: " world".to_string(),
                    }],
                }),
                finish_reason: None,
                safety_ratings: None,
                index: None,
            }],
            usage_metadata: None,
            prompt_feedback: None,
        },
        GeminiStreamChunk {
            candidates: vec![Candidate {
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![GeminiPart::Text {
                        text: "!".to_string(),
                    }],
                }),
                finish_reason: Some("STOP".to_string()),
                safety_ratings: None,
                index: None,
            }],
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: None,
                candidates_token_count: Some(5),
                total_token_count: Some(15),
            }),
            prompt_feedback: None,
        },
    ];

    c.bench_function("sse_event_generation", |b| {
        b.iter(|| {
            let mut event_gen = SSEEventGenerator::new("gemini-3-pro-preview".to_string());
            for chunk in &chunks {
                black_box(event_gen.generate_events(chunk.clone()));
            }
        });
    });
}

fn benchmark_end_to_end(c: &mut Criterion) {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Hello, world!".to_string()),
        }],
        system: Some(SystemPrompt::Text("You are helpful.".to_string())),
        max_tokens: Some(100),
        temperature: Some(0.7),
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: None,
    };

    let gemini_stream = br#"[{"candidates":[{"content":{"parts":[{"text":"Hello!"}],"role":"model"}}],"usageMetadata":{"promptTokenCount":5}},{"candidates":[{"finishReason":"STOP"}]}]"#;

    c.bench_function("end_to_end_transformation", |b| {
        b.iter(|| {
            // Request transformation
            validate_claude_request(&req).unwrap();
            let gemini_req = transform_request(req.clone()).unwrap();
            let _serialized = serde_json::to_vec(&gemini_req).unwrap();

            // Response transformation
            let mut parser = StreamingJsonParser::new();
            let chunks = parser.feed(gemini_stream).unwrap();

            let mut event_gen = SSEEventGenerator::new("gemini-3-pro-preview".to_string());
            for chunk in chunks {
                black_box(event_gen.generate_events(chunk));
            }
        });
    });
}

criterion_group!(
    benches,
    benchmark_model_mapping,
    benchmark_request_validation,
    benchmark_request_transformation,
    benchmark_json_serialization,
    benchmark_streaming_parser,
    benchmark_streaming_parser_incremental,
    benchmark_sse_generation,
    benchmark_end_to_end
);
criterion_main!(benches);
