use crate::params::*;
//use std::collections::{HashMap, HashSet};
//use std::hash::{BuildHasherDefault, Hash, Hasher};
use smallvec::SmallVec;
use crate::types::*;
use fxhash::FxHashMap;
use fxhash::FxHashSet;
use log::*;

//Used in search, but not in dist,triangle
pub fn screen_refs_filenames<'a>(
    identity: f64,
    kmer_to_sketch: &KmerToSketch,
    query_sketch: &Sketch,
    sketch_params: &SketchParams,
    ref_sketches: &'a Vec<Sketch>
) -> Vec<&'a String>{
    let mut count_hash_map = FxHashMap::default();
    for marker in query_sketch.marker_seeds.iter() {
        if kmer_to_sketch.contains_key(marker) {
            for sketch_id in kmer_to_sketch[marker].iter() {
                let count = count_hash_map.entry(sketch_id).or_insert(0);
                *count += 1;
            }
        }
    }
    //Use fixed K value for AA markers, but flexible ones for DNA because saturation less of an
    //issue.
    let k = if sketch_params.use_aa {
        K_MARKER_AA
    } else {
        K_MARKER_DNA
    };
    let cutoff = identity.powi(k as i32);
    trace!("cutoff screening val {}",cutoff);
    let ret = count_hash_map
        .iter()
        .filter(|x| {
            *x.1 > usize::max((cutoff 
                * usize::min(
                    ref_sketches[**x.0 as usize].marker_seeds.len(),
                    query_sketch.marker_seeds.len(),
                ) as f64) as usize,1)
        })
        .map(|x| &ref_sketches[**x.0 as usize].file_name)
        .collect();
    ret

}

//used in triangle, dist, but not search. Note the different behavior when the input sketch
//is small. 
pub fn screen_refs(
    identity: f64,
    kmer_to_sketch: &KmerToSketch,
    query_sketch: &Sketch,
    sketch_params: &SketchParams,
    ref_sketches: &Vec<Sketch>,
) -> FxHashSet<usize> {
    let mut count_hash_map = FxHashMap::default();
    //Don't screen when the input sketch is too small.
    if query_sketch.marker_seeds.len() < 20{
        return (0..ref_sketches.len()).collect();
    }
    for marker in query_sketch.marker_seeds.iter() {
        if kmer_to_sketch.contains_key(marker) {
            for sketch_id in kmer_to_sketch[marker].iter() {
                let count = count_hash_map.entry(sketch_id).or_insert(0);
                *count += 1;
            }
        }
    }
    //Use fixed K value for AA markers, but flexible ones for DNA because saturation less of an
    //issue.
    let k = if sketch_params.use_aa {
        K_MARKER_AA
    } else {
        K_MARKER_DNA
    };
    let cutoff = identity.powi(k as i32);
    let ret = count_hash_map
        .iter()
        .filter(|x| {
            *x.1 > usize::max((cutoff 
                * usize::min(
                    ref_sketches[**x.0 as usize].marker_seeds.len(),
                    query_sketch.marker_seeds.len(),
                ) as f64) as usize,1)
        })
        .map(|x| **x.0 as usize)
        .collect();
    ret
}
pub fn kmer_to_sketch_from_refs(ref_sketches: &Vec<Sketch>) -> KmerToSketch {
//    let max_size: usize = ref_sketches.iter().map(|x| x.marker_seeds.len()).sum::<usize>();
    let mut ret = KmerToSketch::default();
    //ret.reserve(max_size);
    for (i, ref_sketch) in ref_sketches.iter().enumerate() {
        for kmer in ref_sketch.marker_seeds.iter() {
            let sketch_set = ret.entry(*kmer).or_insert(SmallVec::<[u32; KMER_SK_SMALL_VEC_SIZE]>::new());
            sketch_set.push(i as u32);
        }
    }

//    debug!("{} unique marker k-mers found", ret.len());
//    let mut kmer_stats = vec![];
//    for kmer in ret.keys(){
//        kmer_stats.push(ret[kmer].len());
//    }
//    kmer_stats.sort_unstable();
//    let l = kmer_stats.len();
//    debug!("{} - 10, {} - 50, {} - MAX", kmer_stats[l*1/10], kmer_stats[l/2], kmer_stats[l-1]);
    ret
}
