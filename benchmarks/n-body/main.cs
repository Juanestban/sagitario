// n-body: the orbits of the Jovian planets, as the Computer Language Benchmarks Game writes it.
// Numeric loops over an array of structs. The argument is the number of steps.
using System;
using System.Globalization;

struct Body
{
    public double X, Y, Z, Vx, Vy, Vz, Mass;
}

static class Program
{
    const double Pi = 3.141592653589793;
    const double SolarMass = 4.0 * Pi * Pi;
    const double DaysPerYear = 365.24;

    static Body MakeBody(double x, double y, double z, double vx, double vy, double vz, double mass) =>
        new Body { X = x, Y = y, Z = z, Vx = vx * DaysPerYear, Vy = vy * DaysPerYear, Vz = vz * DaysPerYear, Mass = mass * SolarMass };

    static double Energy(Body[] bodies)
    {
        double e = 0.0;
        for (int i = 0; i < 5; i++)
        {
            Body b = bodies[i];
            e += 0.5 * b.Mass * (b.Vx * b.Vx + b.Vy * b.Vy + b.Vz * b.Vz);
            for (int j = i + 1; j < 5; j++)
            {
                Body other = bodies[j];
                double dx = b.X - other.X, dy = b.Y - other.Y, dz = b.Z - other.Z;
                e -= b.Mass * other.Mass / Math.Sqrt(dx * dx + dy * dy + dz * dz);
            }
        }
        return e;
    }

    static void Main(string[] args)
    {
        long steps = args.Length > 0 ? long.Parse(args[0]) : 1000;
        Body[] bodies =
        {
            MakeBody(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0),
            MakeBody(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01, 1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04),
            MakeBody(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01, -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04),
            MakeBody(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01, 2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05),
            MakeBody(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01, 2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05),
        };

        // The sun takes the momentum of the planets, so the center of mass stays still.
        double px = 0.0, py = 0.0, pz = 0.0;
        foreach (Body b in bodies)
        {
            px += b.Vx * b.Mass;
            py += b.Vy * b.Mass;
            pz += b.Vz * b.Mass;
        }
        bodies[0].Vx = -px / SolarMass;
        bodies[0].Vy = -py / SolarMass;
        bodies[0].Vz = -pz / SolarMass;

        Console.WriteLine(Energy(bodies).ToString("F9", CultureInfo.InvariantCulture));

        for (long step = 0; step < steps; step++)
        {
            for (int i = 0; i < 5; i++)
            {
                for (int j = i + 1; j < 5; j++)
                {
                    double dx = bodies[i].X - bodies[j].X;
                    double dy = bodies[i].Y - bodies[j].Y;
                    double dz = bodies[i].Z - bodies[j].Z;
                    double distance2 = dx * dx + dy * dy + dz * dz;
                    double magnitude = 0.01 / (distance2 * Math.Sqrt(distance2));
                    double massI = bodies[i].Mass, massJ = bodies[j].Mass;
                    bodies[i].Vx -= dx * massJ * magnitude;
                    bodies[i].Vy -= dy * massJ * magnitude;
                    bodies[i].Vz -= dz * massJ * magnitude;
                    bodies[j].Vx += dx * massI * magnitude;
                    bodies[j].Vy += dy * massI * magnitude;
                    bodies[j].Vz += dz * massI * magnitude;
                }
            }
            for (int i = 0; i < 5; i++)
            {
                bodies[i].X += 0.01 * bodies[i].Vx;
                bodies[i].Y += 0.01 * bodies[i].Vy;
                bodies[i].Z += 0.01 * bodies[i].Vz;
            }
        }

        Console.WriteLine(Energy(bodies).ToString("F9", CultureInfo.InvariantCulture));
    }
}
