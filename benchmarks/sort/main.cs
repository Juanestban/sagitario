// sort: sorts the argument's number of integers from the generator MINSTD
// (`x = x * 48271 % 2147483647`), the same in every language, with the standard sort of each.
using System;
using System.Collections.Generic;

static class Program
{
    static void Main(string[] args)
    {
        int count = args.Length > 0 ? int.Parse(args[0]) : 1000;
        var values = new List<long>();
        long seed = 42;

        for (int i = 0; i < count; i++)
        {
            seed = seed * 48271 % 2147483647;
            values.Add(seed);
        }

        values.Sort();

        long checksum = 0;
        for (int i = 0; i < count; i += 1000)
        {
            checksum += values[i];
        }

        Console.WriteLine($"min {values[0]} max {values[count - 1]} checksum {checksum}");
    }
}
