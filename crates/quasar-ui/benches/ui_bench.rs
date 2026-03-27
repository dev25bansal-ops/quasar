use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_layout_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout");

    #[derive(Clone, Debug)]
    struct UiNode {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        children: Vec<UiNode>,
    }

    impl UiNode {
        fn new(width: f32, height: f32) -> Self {
            Self {
                x: 0.0,
                y: 0.0,
                width,
                height,
                children: Vec::new(),
            }
        }

        fn with_child(mut self, child: UiNode) -> Self {
            self.children.push(child);
            self
        }
    }

    fn calculate_layout(node: &mut UiNode, x: f32, y: f32) {
        node.x = x;
        node.y = y;

        let mut child_y = y;
        for child in &mut node.children {
            calculate_layout(child, x, child_y);
            child_y += child.height;
        }
    }

    fn build_tree(depth: usize, breadth: usize) -> UiNode {
        if depth == 0 {
            return UiNode::new(100.0, 50.0);
        }

        let mut node = UiNode::new(100.0, 50.0 * breadth as f32);
        for _ in 0..breadth {
            node.children.push(build_tree(depth - 1, breadth));
        }
        node
    }

    for depth in [2, 4, 6].iter() {
        let tree = build_tree(*depth, 3);
        group.bench_with_input(BenchmarkId::new("vertical_layout", depth), depth, |b, _| {
            b.iter(|| {
                let mut tree = tree.clone();
                calculate_layout(black_box(&mut tree), 0.0, 0.0);
            });
        });
    }

    group.finish();
}

fn bench_text_measurement(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_measurement");

    let texts = [
        "Hello, World!",
        "The quick brown fox jumps over the lazy dog.",
        "This is a longer piece of text that would be used in a real UI application for displaying information to the user.",
    ];

    for (i, text) in texts.iter().enumerate() {
        let text = text.to_string();
        group.bench_with_input(BenchmarkId::new("measure_text", i), &text, |b, text| {
            b.iter(|| {
                let len = black_box(text).len();
                let width = len as f32 * 10.0;
                let height = 16.0;
                (width, height)
            });
        });
    }

    group.finish();
}

fn bench_event_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_handling");

    #[derive(Clone, Debug)]
    struct ClickEvent {
        x: f32,
        y: f32,
    }

    #[derive(Clone, Debug)]
    struct UiNode {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        clicked: bool,
    }

    fn hit_test(node: &UiNode, event: &ClickEvent) -> bool {
        event.x >= node.x
            && event.x <= node.x + node.width
            && event.y >= node.y
            && event.y <= node.y + node.height
    }

    let nodes: Vec<UiNode> = (0..100)
        .map(|i| UiNode {
            x: (i % 10) as f32 * 110.0,
            y: (i / 10) as f32 * 60.0,
            width: 100.0,
            height: 50.0,
            clicked: false,
        })
        .collect();

    let event = ClickEvent { x: 55.0, y: 25.0 };

    group.bench_function("hit_test_100_nodes", |b| {
        b.iter(|| {
            for node in &nodes {
                if hit_test(black_box(node), black_box(&event)) {
                    return true;
                }
            }
            false
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_layout_calculation,
    bench_text_measurement,
    bench_event_handling,
);
criterion_main!(benches);
