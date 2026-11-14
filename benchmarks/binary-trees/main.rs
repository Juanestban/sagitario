// binary-trees: builds and walks many complete binary trees, as the Computer Language Benchmarks
// Game writes it. Allocation and reference counting. The argument is the depth of the largest.

struct Node {
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}

impl Node {
    /// The number of nodes of the tree.
    fn check(&self) -> u64 {
        let left = self.left.as_ref().map_or(0, |node| node.check());
        let right = self.right.as_ref().map_or(0, |node| node.check());

        1 + left + right
    }
}

fn tree(depth: u32) -> Box<Node> {
    if depth == 0 {
        return Box::new(Node { left: None, right: None });
    }

    Box::new(Node { left: Some(tree(depth - 1)), right: Some(tree(depth - 1)) })
}

fn main() {
    let requested: u32 = std::env::args().nth(1).and_then(|arg| arg.parse().ok()).unwrap_or(10);
    let min_depth = 4u32;
    let max_depth = if requested < min_depth + 2 { min_depth + 2 } else { requested };

    println!("stretch tree of depth {}\t check: {}", max_depth + 1, tree(max_depth + 1).check());

    let long_lived = tree(max_depth);
    let mut depth = min_depth;

    while depth <= max_depth {
        let iterations = 1u64 << (max_depth - depth + min_depth);
        let mut check = 0u64;

        for _ in 0..iterations {
            check += tree(depth).check();
        }

        println!("{}\t trees of depth {}\t check: {}", iterations, depth, check);
        depth += 2;
    }

    println!("long lived tree of depth {}\t check: {}", max_depth, long_lived.check());
}
