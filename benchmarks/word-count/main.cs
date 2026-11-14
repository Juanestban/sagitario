// word-count: counts the words of a text and prints the most frequent. Strings and a map. The
// text has the argument's number of words, chosen from 2000 by the generator MINSTD
// (`x = x * 48271 % 2147483647`), the same in every language.
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;

static class Program
{
    static void Main(string[] args)
    {
        long count = args.Length > 0 ? long.Parse(args[0]) : 1000;
        var text = new StringBuilder();
        long seed = 42;

        for (long i = 0; i < count; i++)
        {
            seed = seed * 48271 % 2147483647;

            if (i > 0)
            {
                text.Append(' ');
            }

            text.Append('w').Append(seed % 2000);
        }

        var counts = new Dictionary<string, long>();

        foreach (string word in text.ToString().Split(' '))
        {
            counts[word] = counts.TryGetValue(word, out long n) ? n + 1 : 1;
        }

        // The most frequent first, and words of the same count in the order of their bytes.
        var entries = counts.ToList();
        entries.Sort((a, b) => a.Value != b.Value ? b.Value.CompareTo(a.Value) : string.CompareOrdinal(a.Key, b.Key));

        Console.WriteLine($"words {count} distinct {counts.Count}");

        foreach (var entry in entries.Take(5))
        {
            Console.WriteLine($"{entry.Key} {entry.Value}");
        }
    }
}
