// binary-trees: builds and walks many complete binary trees, as the Computer Language Benchmarks
// Game writes it. Allocation and garbage collection. The argument is the depth of the largest.
using System;

sealed class Node
{
    readonly Node? left;
    readonly Node? right;

    public Node(Node? left, Node? right)
    {
        this.left = left;
        this.right = right;
    }

    // The number of nodes of the tree.
    public ulong Check() => 1 + (left?.Check() ?? 0) + (right?.Check() ?? 0);
}

static class Program
{
    static Node Tree(uint depth) => depth == 0 ? new Node(null, null) : new Node(Tree(depth - 1), Tree(depth - 1));

    static void Main(string[] args)
    {
        uint requested = args.Length > 0 ? uint.Parse(args[0]) : 10;
        uint minDepth = 4;
        uint maxDepth = requested < minDepth + 2 ? minDepth + 2 : requested;

        Console.WriteLine($"stretch tree of depth {maxDepth + 1}\t check: {Tree(maxDepth + 1).Check()}");

        Node longLived = Tree(maxDepth);

        for (uint depth = minDepth; depth <= maxDepth; depth += 2)
        {
            ulong iterations = 1UL << (int)(maxDepth - depth + minDepth);
            ulong check = 0;

            for (ulong i = 0; i < iterations; i++)
            {
                check += Tree(depth).Check();
            }

            Console.WriteLine($"{iterations}\t trees of depth {depth}\t check: {check}");
        }

        Console.WriteLine($"long lived tree of depth {maxDepth}\t check: {longLived.Check()}");
    }
}
