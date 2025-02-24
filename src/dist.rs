use crate::chain;
use crate::regression;
use crate::file_io;
use crate::params::*;
use crate::screen;
use crate::types::*;
use log::*;
use rayon::prelude::*;
use std::sync::Mutex;
use std::time::Instant;

pub fn dist(command_params: CommandParams, mut sketch_params: SketchParams) {
    let ref_sketches;
    let query_params;
    let query_sketches;
    let now = Instant::now();
    if command_params.refs_are_sketch {
        let new_sketch_params;
        info!("Sketches detected.");
        (new_sketch_params, ref_sketches) = file_io::sketches_from_sketch(
            &command_params.ref_files,
        );
        if new_sketch_params != sketch_params {
            warn!("Parameters from .sketch files not equal to the input parameters. Using parameters from .sketch files.")
        }
        sketch_params = new_sketch_params;
    } else if command_params.individual_contig_r {
        ref_sketches = file_io::fastx_to_multiple_sketch_rewrite(
            &command_params.ref_files,
            &sketch_params,
            true,
        );
    } else {
        ref_sketches =
            file_io::fastx_to_sketches(&command_params.ref_files, &sketch_params, true);
    }
    if command_params.queries_are_sketch {
        (query_params, query_sketches) =
            file_io::sketches_from_sketch(&command_params.query_files);
        if sketch_params != query_params && command_params.refs_are_sketch {
            panic!(
                "Query sketch parameters were not equal to reference sketch parameters. Exiting."
            );
        } else if sketch_params != query_params {
            warn!("Parameters from .sketch files not equal to the input parameters. Using parameters from .sketch files.")
        }
    } else if command_params.individual_contig_q {
        query_sketches = file_io::fastx_to_multiple_sketch_rewrite(
            &command_params.query_files,
            &sketch_params,
            true,
        );
    } else {
        query_sketches =
            file_io::fastx_to_sketches(&command_params.query_files, &sketch_params, true);
    }
    if query_sketches.is_empty() || ref_sketches.is_empty() {
        error!("No reference sketches/genomes or query sketches/genomes found.");
        std::process::exit(1)
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

    let kmer_to_sketch;

    if command_params.screen {
        let now = Instant::now();
        info!("Full index option detected; generating marker hash table");
        kmer_to_sketch = screen::kmer_to_sketch_from_refs(&ref_sketches);
        info!("Full indexing time: {}", now.elapsed().as_secs_f32());
    } else {
        kmer_to_sketch = KmerToSketch::default();
    }

    info!("Generating sketch time: {}", now.elapsed().as_secs_f32());
    let now = Instant::now();
    let js = (0..query_sketches.len())
        .into_iter()
        .collect::<Vec<usize>>();
    let anis: Mutex<Vec<AniEstResult>> = Mutex::new(vec![]);
    let counter: Mutex<usize> = Mutex::new(0);
    let first_write: Mutex<bool> = Mutex::new(true);
    js.into_par_iter().for_each(|j| {
        let query_sketch = &query_sketches[j];
        if !command_params.screen {
            let is = (0..ref_sketches.len()).into_iter().collect::<Vec<usize>>();
            is.into_par_iter().for_each(|i| {
                let ref_sketch = &ref_sketches[i];
                let passed_screen =
                    chain::check_markers_quickly(query_sketch, ref_sketch, screen_val);
                if passed_screen {
                    let map_params = chain::map_params_from_sketch(
                        ref_sketch,
                        sketch_params.use_aa,
                        &command_params,
                    );
                    let ani_res;
                    if map_params != MapParams::default() {
                        ani_res = chain::chain_seeds(ref_sketch, query_sketch, map_params);
                    } else {
                        ani_res = AniEstResult::default();
                    }
                    if ani_res.ani > 0.1 {
                        let mut locked = anis.lock().unwrap();
                        locked.push(ani_res);
                    }
                }
            });
        } else {
            let refs_passing_screen_table = screen::screen_refs(
                screen_val,
                &kmer_to_sketch,
                query_sketch,
                &sketch_params,
                &ref_sketches,
            );
            refs_passing_screen_table.into_par_iter().for_each(|i| {
                let ref_sketch = &ref_sketches[i];
                let map_params = chain::map_params_from_sketch(
                    ref_sketch,
                    sketch_params.use_aa,
                    &command_params,
                );
                let ani_res = chain::chain_seeds(ref_sketch, query_sketch, map_params);
                if ani_res.ani > 0.1{
                    let mut locked = anis.lock().unwrap();
                    locked.push(ani_res);
                }
            });
        }
        let c;
        {
            let mut locked = counter.lock().unwrap();
            *locked += 1;
            c = *locked;
        }
        if c % 100 == 0 && c != 0{
            info!("{} query sequences processed.", c);
            if c % INTERMEDIATE_WRITE_COUNT == 0 && c != 0{
                info!("Writing results for {} query sequences.", INTERMEDIATE_WRITE_COUNT);
                let moved_anis: Vec<AniEstResult>;
                {
                let mut locked = anis.lock().unwrap();
                moved_anis = std::mem::take(&mut locked);
                }
                let mut fw = first_write.lock().unwrap();
                file_io::write_query_ref_list(
                    &moved_anis,
                    &command_params.out_file_name,
                    command_params.max_results,
                    sketch_params.use_aa,
                    command_params.est_ci,
                    command_params.detailed_out,
                    !*fw
                );
                if *fw == true{
                    *fw = false;
                }
            }
        }
    });
    let mut anis = anis.into_inner().unwrap();
    let model_opt = regression::get_model(sketch_params.c, command_params.learned_ani);
    if model_opt.is_some(){
        info!("{}",LEARNED_INFO_HELP);
        let model = model_opt.as_ref().unwrap();
        for ani in anis.iter_mut(){
            regression::predict_from_ani_res(ani, &model);
        }
    }
    file_io::write_query_ref_list(
        &anis,
        &command_params.out_file_name,
        command_params.max_results,
        sketch_params.use_aa,
        command_params.est_ci,
        command_params.detailed_out,
        !*first_write.lock().unwrap()
    );
    info!("ANI calculation time: {}", now.elapsed().as_secs_f32());
}
