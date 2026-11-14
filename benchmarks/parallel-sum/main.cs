// parallel-sum: four threads add `i * i % 1000` over their quarter of `0..n`, and the main thread
// adds their results. Threads and joins. The argument is `n`.
using System;
using System.Threading;

static class Program
{
    const long Threads = 4;

    static long Partial(long start, long end)
    {
        long sum = 0;
        for (long i = start; i < end; i++)
        {
            sum += i * i % 1000;
        }
        return sum;
    }

    static void Main(string[] args)
    {
        long n = args.Length > 0 ? long.Parse(args[0]) : 1000;
        long chunk = n / Threads;
        var sums = new long[Threads];
        var threads = new Thread[Threads];

        for (long index = 0; index < Threads; index++)
        {
            long slot = index;
            long start = index * chunk;
            long end = index == Threads - 1 ? n : start + chunk;
            threads[index] = new Thread(() => sums[slot] = Partial(start, end));
            threads[index].Start();
        }

        long total = 0;
        for (long index = 0; index < Threads; index++)
        {
            threads[index].Join();
            total += sums[index];
        }

        Console.WriteLine(total);
    }
}
