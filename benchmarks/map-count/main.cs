// map-count: counts how often each of 100000 keys comes, in a hash map of integers. The argument
// is the number of keys drawn by the generator MINSTD (`x = x * 48271 % 2147483647`), the same
// in every language.
using System;
using System.Collections.Generic;

static class Program
{
    static void Main(string[] args)
    {
        long count = args.Length > 0 ? long.Parse(args[0]) : 1000;
        var counts = new Dictionary<long, long>();
        long seed = 42;

        for (long i = 0; i < count; i++)
        {
            seed = seed * 48271 % 2147483647;
            long key = seed % 100000;
            counts[key] = counts.TryGetValue(key, out long n) ? n + 1 : 1;
        }

        // The largest count, at its smallest key.
        long most = 0, mostKey = 0;
        foreach (var entry in counts)
        {
            if (entry.Value > most || (entry.Value == most && entry.Key < mostKey))
            {
                most = entry.Value;
                mostKey = entry.Key;
            }
        }

        long small = 0;
        for (long key = 0; key < 100; key++)
        {
            small += counts.TryGetValue(key, out long n) ? n : 0;
        }

        Console.WriteLine($"keys {counts.Count} most {most} at {mostKey} small {small}");
    }
}
