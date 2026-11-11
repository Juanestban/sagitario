// spectral-norm: the largest eigenvalue of an infinite matrix by the power method, as the Computer
// Language Benchmarks Game writes it. Numeric loops over arrays. The argument is the size.
using System;
using System.Globalization;

static class Program
{
    static double A(long i, long j) => 1.0 / ((i + j) * (i + j + 1) / 2 + i + 1);

    static void MulAv(double[] v, double[] output)
    {
        int n = v.Length;
        for (int i = 0; i < n; i++)
        {
            double sum = 0.0;
            for (int j = 0; j < n; j++)
            {
                sum += A(i, j) * v[j];
            }
            output[i] = sum;
        }
    }

    static void MulAtv(double[] v, double[] output)
    {
        int n = v.Length;
        for (int i = 0; i < n; i++)
        {
            double sum = 0.0;
            for (int j = 0; j < n; j++)
            {
                sum += A(j, i) * v[j];
            }
            output[i] = sum;
        }
    }

    static void MulAtAv(double[] v, double[] output, double[] tmp)
    {
        MulAv(v, tmp);
        MulAtv(tmp, output);
    }

    static void Main(string[] args)
    {
        int n = args.Length > 0 ? int.Parse(args[0]) : 100;
        double[] u = new double[n];
        double[] v = new double[n];
        double[] tmp = new double[n];
        Array.Fill(u, 1.0);

        for (int step = 0; step < 10; step++)
        {
            MulAtAv(u, v, tmp);
            MulAtAv(v, u, tmp);
        }

        double vbv = 0.0, vv = 0.0;
        for (int i = 0; i < n; i++)
        {
            vbv += u[i] * v[i];
            vv += v[i] * v[i];
        }

        Console.WriteLine(Math.Sqrt(vbv / vv).ToString("F9", CultureInfo.InvariantCulture));
    }
}
