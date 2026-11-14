// n-body: the orbits of the Jovian planets, as the Computer Language Benchmarks Game writes it.
// Numeric loops over an array of structs. The argument is the number of steps.

const PI: f64 = 3.141592653589793;
const SOLAR_MASS: f64 = 4.0 * PI * PI;
const DAYS_PER_YEAR: f64 = 365.24;

#[derive(Clone, Copy)]
struct Body {
    x: f64,
    y: f64,
    z: f64,
    vx: f64,
    vy: f64,
    vz: f64,
    mass: f64,
}

fn body(x: f64, y: f64, z: f64, vx: f64, vy: f64, vz: f64, mass: f64) -> Body {
    Body { x, y, z, vx: vx * DAYS_PER_YEAR, vy: vy * DAYS_PER_YEAR, vz: vz * DAYS_PER_YEAR, mass: mass * SOLAR_MASS }
}

fn energy(bodies: &[Body; 5]) -> f64 {
    let mut e = 0.0;

    for i in 0..5 {
        let b = bodies[i];

        e += 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);

        for j in i + 1..5 {
            let other = bodies[j];
            let dx = b.x - other.x;
            let dy = b.y - other.y;
            let dz = b.z - other.z;

            e -= b.mass * other.mass / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }

    e
}

fn main() {
    let steps: u64 = std::env::args().nth(1).and_then(|arg| arg.parse().ok()).unwrap_or(1000);
    let mut bodies = [
        body(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0),
        body(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01, 1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04),
        body(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01, -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04),
        body(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01, 2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05),
        body(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01, 2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05),
    ];

    // The sun takes the momentum of the planets, so the center of mass stays still.
    let (mut px, mut py, mut pz) = (0.0, 0.0, 0.0);

    for b in &bodies {
        px += b.vx * b.mass;
        py += b.vy * b.mass;
        pz += b.vz * b.mass;
    }

    bodies[0].vx = -px / SOLAR_MASS;
    bodies[0].vy = -py / SOLAR_MASS;
    bodies[0].vz = -pz / SOLAR_MASS;

    println!("{:.9}", energy(&bodies));

    for _ in 0..steps {
        for i in 0..5 {
            for j in i + 1..5 {
                let dx = bodies[i].x - bodies[j].x;
                let dy = bodies[i].y - bodies[j].y;
                let dz = bodies[i].z - bodies[j].z;
                let distance2 = dx * dx + dy * dy + dz * dz;
                let magnitude = 0.01 / (distance2 * distance2.sqrt());
                let mass_i = bodies[i].mass;
                let mass_j = bodies[j].mass;

                bodies[i].vx -= dx * mass_j * magnitude;
                bodies[i].vy -= dy * mass_j * magnitude;
                bodies[i].vz -= dz * mass_j * magnitude;
                bodies[j].vx += dx * mass_i * magnitude;
                bodies[j].vy += dy * mass_i * magnitude;
                bodies[j].vz += dz * mass_i * magnitude;
            }
        }

        for b in bodies.iter_mut() {
            b.x += 0.01 * b.vx;
            b.y += 0.01 * b.vy;
            b.z += 0.01 * b.vz;
        }
    }

    println!("{:.9}", energy(&bodies));
}
