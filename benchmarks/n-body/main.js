// n-body: the orbits of the Jovian planets, as the Computer Language Benchmarks Game writes it.
// Numeric loops over an array of objects. The argument is the number of steps.
"use strict";

const PI = 3.141592653589793;
const SOLAR_MASS = 4.0 * PI * PI;
const DAYS_PER_YEAR = 365.24;

function body(x, y, z, vx, vy, vz, mass) {
  return { x, y, z, vx: vx * DAYS_PER_YEAR, vy: vy * DAYS_PER_YEAR, vz: vz * DAYS_PER_YEAR, mass: mass * SOLAR_MASS };
}

function energy(bodies) {
  let e = 0.0;
  for (let i = 0; i < 5; i++) {
    const b = bodies[i];
    e += 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);
    for (let j = i + 1; j < 5; j++) {
      const other = bodies[j];
      const dx = b.x - other.x;
      const dy = b.y - other.y;
      const dz = b.z - other.z;
      e -= b.mass * other.mass / Math.sqrt(dx * dx + dy * dy + dz * dz);
    }
  }
  return e;
}

function main() {
  const steps = process.argv.length > 2 ? Number(process.argv[2]) : 1000;
  const bodies = [
    body(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0),
    body(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01, 1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04),
    body(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01, -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04),
    body(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01, 2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05),
    body(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01, 2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05),
  ];

  // The sun takes the momentum of the planets, so the center of mass stays still.
  let px = 0.0, py = 0.0, pz = 0.0;
  for (const b of bodies) {
    px += b.vx * b.mass;
    py += b.vy * b.mass;
    pz += b.vz * b.mass;
  }
  bodies[0].vx = -px / SOLAR_MASS;
  bodies[0].vy = -py / SOLAR_MASS;
  bodies[0].vz = -pz / SOLAR_MASS;

  console.log(energy(bodies).toFixed(9));

  for (let step = 0; step < steps; step++) {
    for (let i = 0; i < 5; i++) {
      for (let j = i + 1; j < 5; j++) {
        const dx = bodies[i].x - bodies[j].x;
        const dy = bodies[i].y - bodies[j].y;
        const dz = bodies[i].z - bodies[j].z;
        const distance2 = dx * dx + dy * dy + dz * dz;
        const magnitude = 0.01 / (distance2 * Math.sqrt(distance2));
        const massI = bodies[i].mass;
        const massJ = bodies[j].mass;
        bodies[i].vx -= dx * massJ * magnitude;
        bodies[i].vy -= dy * massJ * magnitude;
        bodies[i].vz -= dz * massJ * magnitude;
        bodies[j].vx += dx * massI * magnitude;
        bodies[j].vy += dy * massI * magnitude;
        bodies[j].vz += dz * massI * magnitude;
      }
    }
    for (const b of bodies) {
      b.x += 0.01 * b.vx;
      b.y += 0.01 * b.vy;
      b.z += 0.01 * b.vz;
    }
  }

  console.log(energy(bodies).toFixed(9));
}

main();
