#![cfg_attr(feature = "nightly", feature(stdarch_x86_avx512))]
#![cfg_attr(feature = "nightly", feature(avx512_target_feature))]

#[cfg(all(feature = "nightly", target_arch = "x86_64"))]
mod x86 {
    use aligned_vec::avec;
    use diol::prelude::*;

    use core::arch::x86_64::*;
    use pulp::x86::V4;
    use pulp::{f64x8, Simd};

    fn sum_scalar(bencher: Bencher, len: usize) {
        let v = &*avec![0.0f64; len];

        bencher.bench(|| black_box(v.iter().sum::<f64>()));
    }

    #[target_feature(enable = "avx512f")]
    unsafe fn sum_stdarch_imp(v: &[f64]) -> f64 {
        let mut acc0 = _mm512_set1_pd(0.0);
        let mut acc1 = _mm512_set1_pd(0.0);
        let mut acc2 = _mm512_set1_pd(0.0);
        let mut acc3 = _mm512_set1_pd(0.0);

        let (head, tail) = pulp::as_arrays::<8, _>(v);
        let (head4, head1) = pulp::as_arrays::<4, _>(head);

        for [x0, x1, x2, x3] in head4 {
            let x0 = pulp::cast(*x0);
            let x1 = pulp::cast(*x1);
            let x2 = pulp::cast(*x2);
            let x3 = pulp::cast(*x3);

            acc0 = _mm512_add_pd(acc0, x0);
            acc1 = _mm512_add_pd(acc1, x1);
            acc2 = _mm512_add_pd(acc2, x2);
            acc3 = _mm512_add_pd(acc3, x3);
        }

        for x0 in head1 {
            let x0 = pulp::cast(*x0);
            acc0 = _mm512_add_pd(acc0, x0);
        }

        acc0 = _mm512_add_pd(acc0, acc1);
        acc2 = _mm512_add_pd(acc2, acc3);
        acc0 = _mm512_add_pd(acc0, acc2);

        let acc: [__m256d; 2] = pulp::cast(acc0);
        let acc = _mm256_add_pd(acc[0], acc[1]);

        let acc: [__m128d; 2] = pulp::cast(acc);
        let acc = _mm_add_pd(acc[0], acc[1]);

        let acc: [f64; 2] = pulp::cast(acc);
        let mut acc = acc[0] + acc[1];

        for x0 in tail {
            acc += *x0;
        }
        acc
    }

    fn sum_stdarch(bencher: Bencher, len: usize) {
        let v = &*avec![0.0f64; len];

        bencher.bench(|| unsafe { black_box(sum_stdarch_imp(v)) });
    }

    fn sum_pulp(bencher: Bencher, len: usize) {
        let simd = V4::try_new().unwrap();
        let v = &*avec![0.0f64; len];

        bencher.bench(|| {
            struct Imp<'a> {
                simd: V4,
                v: &'a [f64],
            }

            impl pulp::NullaryFnOnce for Imp<'_> {
                type Output = f64;

                #[inline(always)]
                fn call(self) -> Self::Output {
                    let Self { simd, v } = self;

                    let (head, tail) = pulp::as_arrays::<8, _>(v);

                    let tail = simd.partial_load_f64s(tail);

                    simd.reduce_sum_f64s(simd.add_f64s(inline_barrier(simd, head), tail))
                }
            }

            simd.vectorize(Imp { simd, v })
        });
    }

    #[inline(always)]
    fn inline_barrier(simd: V4, head: &[[f64; 8]]) -> f64x8 {
        struct Imp<'a> {
            simd: V4,
            head: &'a [[f64; 8]],
        }

        impl pulp::NullaryFnOnce for Imp<'_> {
            type Output = f64x8;

            #[inline(always)]
            fn call(self) -> Self::Output {
                let Self { simd, head } = self;

                let mut acc0 = simd.splat_f64x8(0.0);
                let mut acc1 = simd.splat_f64x8(0.0);
                let mut acc2 = simd.splat_f64x8(0.0);
                let mut acc3 = simd.splat_f64x8(0.0);

                let (head4, head1) = pulp::as_arrays::<4, _>(head);

                for [x0, x1, x2, x3] in head4 {
                    let x0 = pulp::cast(*x0);
                    let x1 = pulp::cast(*x1);
                    let x2 = pulp::cast(*x2);
                    let x3 = pulp::cast(*x3);

                    acc0 = simd.add_f64x8(acc0, x0);
                    acc1 = simd.add_f64x8(acc1, x1);
                    acc2 = simd.add_f64x8(acc2, x2);
                    acc3 = simd.add_f64x8(acc3, x3);
                }
                for x0 in head1 {
                    let x0 = pulp::cast(*x0);
                    acc0 = simd.add_f64x8(acc0, x0);
                }

                acc0 = simd.add_f64x8(acc0, acc1);
                acc2 = simd.add_f64x8(acc2, acc3);
                acc0 = simd.add_f64x8(acc0, acc2);
                acc0
            }
        }
        simd.vectorize(Imp { simd, head })
    }

    fn sum_pulp_dispatch(bencher: Bencher, len: usize) {
        let v = &*avec![0.0f64; len];

        bencher.bench(|| {
            struct Imp<'a> {
                v: &'a [f64],
            }

            impl pulp::WithSimd for Imp<'_> {
                type Output = f64;

                #[inline(always)]
                fn with_simd<S: Simd>(self, simd: S) -> Self::Output {
                    let Self { v } = self;

                    let (head, tail) = S::as_simd_f64s(v);
                    let tail = simd.partial_load_f64s(tail);

                    simd.reduce_sum_f64s(simd.add_f64s(inline_barrier_generic(simd, head), tail))
                }
            }

            pulp::Arch::new().dispatch(Imp { v })
        });
    }

    #[inline(always)]
    fn inline_barrier_generic<S: Simd>(simd: S, head: &[S::f64s]) -> S::f64s {
        struct Imp<'a, S: Simd> {
            simd: S,
            head: &'a [S::f64s],
        }

        impl<S: Simd> pulp::NullaryFnOnce for Imp<'_, S> {
            type Output = S::f64s;

            #[inline(always)]
            fn call(self) -> Self::Output {
                let Self { simd, head } = self;

                let mut acc0 = simd.splat_f64s(0.0);
                let mut acc1 = simd.splat_f64s(0.0);
                let mut acc2 = simd.splat_f64s(0.0);
                let mut acc3 = simd.splat_f64s(0.0);

                let (head4, head1) = pulp::as_arrays::<4, _>(head);

                for [x0, x1, x2, x3] in head4 {
                    let x0 = pulp::cast(*x0);
                    let x1 = pulp::cast(*x1);
                    let x2 = pulp::cast(*x2);
                    let x3 = pulp::cast(*x3);

                    acc0 = simd.add_f64s(acc0, x0);
                    acc1 = simd.add_f64s(acc1, x1);
                    acc2 = simd.add_f64s(acc2, x2);
                    acc3 = simd.add_f64s(acc3, x3);
                }

                for x0 in head1 {
                    let x0 = pulp::cast(*x0);
                    acc0 = simd.add_f64s(acc0, x0);
                }

                acc0 = simd.add_f64s(acc0, acc1);
                acc2 = simd.add_f64s(acc2, acc3);
                acc0 = simd.add_f64s(acc0, acc2);
                acc0
            }
        }
        simd.vectorize(Imp { simd, head })
    }

    pub fn main() -> std::io::Result<()> {
        let mut bench = Bench::new(BenchConfig::from_args()?);

        bench.register_many(
            list![sum_scalar, sum_stdarch, sum_pulp, sum_pulp_dispatch],
            [1, 2, 3, 4, 8, 15, 16, 32, 64, 256],
        );
        bench.run()?;
        Ok(())
    }
}

fn main() -> std::io::Result<()> {
    #[cfg(all(feature = "nightly", target_arch = "x86_64"))]
    x86::main()?;
    Ok(())
}