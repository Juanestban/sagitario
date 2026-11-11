/* n-body: the orbits of the Jovian planets, as the Computer Language Benchmarks Game writes it.
 * Numeric loops over an array of structs. The argument is the number of steps. */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

#define PI 3.141592653589793
#define SOLAR_MASS (4.0 * PI * PI)
#define DAYS_PER_YEAR 365.24

typedef struct {
  double x, y, z, vx, vy, vz, mass;
} Body;

static Body body(double x, double y, double z, double vx, double vy, double vz, double mass) {
  Body b = {x, y, z, vx * DAYS_PER_YEAR, vy * DAYS_PER_YEAR, vz * DAYS_PER_YEAR, mass * SOLAR_MASS};
  return b;
}

static double energy(const Body *bodies) {
  double e = 0.0;
  for (int i = 0; i < 5; i++) {
    Body b = bodies[i];
    e += 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);
    for (int j = i + 1; j < 5; j++) {
      Body other = bodies[j];
      double dx = b.x - other.x;
      double dy = b.y - other.y;
      double dz = b.z - other.z;
      e -= b.mass * other.mass / sqrt(dx * dx + dy * dy + dz * dz);
    }
  }
  return e;
}

int main(int argc, char **argv) {
  unsigned long long steps = argc > 1 ? strtoull(argv[1], NULL, 10) : 1000;
  Body bodies[5] = {
      body(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0),
      body(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01, 1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05,
           9.54791938424326609e-04),
      body(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01, -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05,
           2.85885980666130812e-04),
      body(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01, 2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05,
           4.36624404335156298e-05),
      body(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01, 2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05,
           5.15138902046611451e-05),
  };

  /* The sun takes the momentum of the planets, so the center of mass stays still. */
  double px = 0.0, py = 0.0, pz = 0.0;
  for (int i = 0; i < 5; i++) {
    px += bodies[i].vx * bodies[i].mass;
    py += bodies[i].vy * bodies[i].mass;
    pz += bodies[i].vz * bodies[i].mass;
  }
  bodies[0].vx = -px / SOLAR_MASS;
  bodies[0].vy = -py / SOLAR_MASS;
  bodies[0].vz = -pz / SOLAR_MASS;

  printf("%.9f\n", energy(bodies));

  for (unsigned long long step = 0; step < steps; step++) {
    for (int i = 0; i < 5; i++) {
      for (int j = i + 1; j < 5; j++) {
        double dx = bodies[i].x - bodies[j].x;
        double dy = bodies[i].y - bodies[j].y;
        double dz = bodies[i].z - bodies[j].z;
        double distance2 = dx * dx + dy * dy + dz * dz;
        double magnitude = 0.01 / (distance2 * sqrt(distance2));
        double mass_i = bodies[i].mass;
        double mass_j = bodies[j].mass;
        bodies[i].vx -= dx * mass_j * magnitude;
        bodies[i].vy -= dy * mass_j * magnitude;
        bodies[i].vz -= dz * mass_j * magnitude;
        bodies[j].vx += dx * mass_i * magnitude;
        bodies[j].vy += dy * mass_i * magnitude;
        bodies[j].vz += dz * mass_i * magnitude;
      }
    }
    for (int i = 0; i < 5; i++) {
      bodies[i].x += 0.01 * bodies[i].vx;
      bodies[i].y += 0.01 * bodies[i].vy;
      bodies[i].z += 0.01 * bodies[i].vz;
    }
  }

  printf("%.9f\n", energy(bodies));
  return 0;
}
