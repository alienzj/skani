use crate::chain;
use crate::file_io;
use crate::params::*;
use crate::regression;
use crate::screen;
use crate::types::*;
use fxhash::FxHashMap;
use log::*;
use rayon::prelude::*;
use std::sync::Mutex;
use std::time::Instant;

pub fn triangle(command_params: CommandParams, mut sketch_params: SketchParams) {
    //TODO
    let ref_sketches;
    let now = Instant::now();
    if command_params.refs_are_sketch {
        info!("Sketches detected.");
        let param_and_sketches = file_io::sketches_from_sketch(&command_params.ref_files);
        if param_and_sketches.0.c != sketch_params.c {
            warn!("Input parameter c = {} is not equal to the sketch parameter c = {}. Using sketch parameters.", sketch_params.c, param_and_sketches.0.c);
        }
        ref_sketches = param_and_sketches.1;
        sketch_params = param_and_sketches.0;
    } else if command_params.individual_contig_r {
        ref_sketches = file_io::fastx_to_multiple_sketch_rewrite(
            &command_params.ref_files,
            &sketch_params,
            true,
        );
    } else {
        ref_sketches = file_io::fastx_to_sketches(&command_params.ref_files, &sketch_params, true);
    }
    let screen_val;
    if command_params.screen_val == 0. {
        if sketch_params.use_aa {
            screen_val = SEARCH_AAI_CUTOFF_DEFAULT;
        } else {
            screen_val = SEARCH_ANI_CUTOFF_DEFAULT;
        }
    } else {
        screen_val = command_params.screen_val;
    }
    let anis: Mutex<FxHashMap<usize, FxHashMap<usize, AniEstResult>>> =
        Mutex::new(FxHashMap::default());

    if ref_sketches.is_empty() {
        error!("No genomes/sketches found.");
        std::process::exit(1)
    }
    let kmer_to_sketch = screen::kmer_to_sketch_from_refs(&ref_sketches);
    let counter: Mutex<usize> = Mutex::new(0);
    let first: Mutex<bool> = Mutex::new(true);

    let model = regression::get_model(sketch_params.c, command_params.learned_ani);
    if model.is_some() {
        info!("{}", LEARNED_INFO_HELP);
    }
    (0..ref_sketches.len() - 1)
        .collect::<Vec<usize>>()
        .into_par_iter()
        .for_each(|i| {
            let ref_sketch_i = &ref_sketches[i];
            //if command_params.screen {
            let screened_refs = screen::screen_refs(
                screen_val,
                &kmer_to_sketch,
                ref_sketch_i,
                &sketch_params,
                &ref_sketches,
            );
            debug!(
                "{} has {} refs passing screening.",
                ref_sketch_i.file_name,
                screened_refs.len()
            );
            screened_refs.into_par_iter().for_each(|j| {
                if j > i {
                    let map_params = chain::map_params_from_sketch(
                        ref_sketch_i,
                        sketch_params.use_aa,
                        &command_params,
                    );
                    let ref_sketch_j = &ref_sketches[j];
                    let mut ani_res = chain::chain_seeds(ref_sketch_i, ref_sketch_j, map_params);
                    if ani_res.ani > 0.1 {
                        if command_params.learned_ani {
                            let gbdt = model.as_ref().unwrap();
                            regression::predict_from_ani_res(&mut ani_res, gbdt);
                        }
                        let mut locked = anis.lock().unwrap();
                        let mapi = locked.entry(i).or_insert(FxHashMap::default());
                        mapi.insert(j, ani_res);
                    }
                }
            });

            let c;
            {
                let mut locked = counter.lock().unwrap();
                *locked += 1;
                c = *locked;
            }
            if c % 100 == 0 && c != 0 {
                info!("{} query sequences processed.", c);
                if c % INTERMEDIATE_WRITE_COUNT == 0 && c != 0 {
                    let moved_anis: FxHashMap<_,_>;
                    {
                        let mut locked = anis.lock().unwrap();
                        moved_anis = std::mem::take(&mut locked);
                        let mut locked = first.lock().unwrap();
                        info!("Writing results for {} query sequences.", INTERMEDIATE_WRITE_COUNT);
                        file_io::write_sparse_matrix(
                            &moved_anis,
                            &ref_sketches,
                            &command_params.out_file_name,
                            sketch_params.use_aa,
                            command_params.est_ci,
                            command_params.detailed_out,
                            !*locked,
                        );
                        if *locked == true {
                            *locked = false;
                        }
                    }
                }
            }

            //
        });
    let anis = anis.into_inner().unwrap();
    let now_pred = Instant::now();

    debug!("Prediction time: {}", now_pred.elapsed().as_secs_f32());

    if command_params.sparse {
        file_io::write_sparse_matrix(
            &anis,
            &ref_sketches,
            &command_params.out_file_name,
            sketch_params.use_aa,
            command_params.est_ci,
            command_params.detailed_out,
            !*first.lock().unwrap(),
        );
    } else {
        file_io::write_phyllip_matrix(
            &anis,
            &ref_sketches,
            &command_params.out_file_name,
            command_params.individual_contig_r,
            command_params.full_matrix,
            sketch_params.use_aa,
            command_params.distance,
        );
    }
    info!("ANI triangle time: {}", now.elapsed().as_secs_f32());
}
