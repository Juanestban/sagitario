// channels: a producer thread sends `0..n` through a channel and the main thread receives and adds
// them. A blocking collection, its lock and its waits. The argument is `n`.
using System;
using System.Collections.Concurrent;
using System.Threading;

static class Program
{
    static void Main(string[] args)
    {
        long n = args.Length > 0 ? long.Parse(args[0]) : 1000;
        var channel = new BlockingCollection<long>();
        var producer = new Thread(() =>
        {
            for (long i = 0; i < n; i++)
            {
                channel.Add(i);
            }
            channel.CompleteAdding();
        });
        producer.Start();

        long count = 0, sum = 0;
        foreach (long value in channel.GetConsumingEnumerable())
        {
            count++;
            sum += value;
        }

        producer.Join();
        Console.WriteLine($"received {count} sum {sum}");
    }
}
