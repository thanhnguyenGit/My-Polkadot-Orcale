use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::fs::File;
use std::io::Read;
use anyhow::{anyhow,Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wasmtime::{Engine, Func, Instance, Module, Store, Val, Linker, Caller, ImportType, Table, Extern, ExternType, ValType, V128, Ref, WasmTy, TypedFunc, Memory};
use parity_scale_codec;
use parity_scale_codec::{Decode, Encode};
use serde_json::Value;
use std::convert::Infallible;
use hyper::Server;
use hyper::service::{make_service_fn, service_fn};

#[derive(Debug, Deserialize)]
struct Root {
    headers : Header,
    functions: Vec<Function>,
    classes: Vec<Class>,
    variables: Vec<Variable>,
}

#[derive(Debug,Deserialize)]
struct Header {
    name : Option<String>,
    header : String,
}

#[derive(Debug, Deserialize)]
struct Function {
    name: String,
    params: Vec<Param>,
    result: String,
    selector: String,
}

#[derive(Debug, Deserialize)]
struct Param {
    name: String,
    #[serde(rename = "type")]
    param_type: String,
}

#[derive(Debug, Deserialize)]
struct Class {
    class_selector : String,
    name: String,
    fields : Vec<Param>,
    methods: Vec<Function>
}

#[derive(Debug, Deserialize)]
struct Variable {
    name: String,
    #[serde(rename = "type")]
    var_type: String,
    selector: String,
}

#[derive(Default)]
struct SelectorRegistry {
    origin : String,
    functions : HashMap<String,Function>,
    variables : HashMap<String,Variable>,
    classes_schema : HashMap<String,Vec<Param>>
}

pub async fn abi_reader(file_content: &str, wasm_bytecode : &[u8]) -> anyhow::Result<Vec<u8>> {
    let root: Root = serde_json::from_str(&file_content)?;

    let mut selector_registry = SelectorRegistry {
        origin: root.headers.header.clone(),
        functions: HashMap::default(),
        variables: HashMap::default(),
        classes_schema : HashMap::default(),
    };

    // assert_eq!(check_header_hash(&root.headers.header,wasm_bytecode_path), true,
    //            "The hash from {:?} is not equal to the header {:?}",abi_path,root.headers.header
    // );

    for (_,value) in root.functions.into_iter().enumerate() {
        selector_registry.functions.insert(value.selector.clone(),value);
    }
    for value in root.classes.into_iter().flat_map(|class| {
        selector_registry.classes_schema.insert(class.name,class.fields);
        class.methods.into_iter()
    }) {
        selector_registry.functions.insert(value.selector.clone(),value);
    }
    for (_,value) in root.variables.into_iter().enumerate() {
        selector_registry.variables.insert(value.selector.clone(),value);
    }

    let res = wasmtime_runner(wasm_bytecode,selector_registry)
        .await
        .expect("DEBUG: wasmtime runner run");
    Ok(res)
}

fn check_header_hash(header : &str,wasm_bytecode_path: &str) -> bool {
    let wasm_hash = {
        let mut header_hasher = Sha256::new();
        let mut wasm_reader = File::open(wasm_bytecode_path)
            .map_err(|e| anyhow!("ERROR: failed to read WASM file: {}", e)).expect("Re check bro 1");
        let mut wasm_content = Vec::new();
        wasm_reader.read_to_end(&mut wasm_content)
            .map_err(|e| anyhow!("ERROR: failed to parse the wasm content to variable: {}", e)).expect("Re check bro 2");
        header_hasher.update(&wasm_content);
        let hash = header_hasher.finalize();
        format!("0x{:x}", hash)
    };
    header.eq(&wasm_hash)
}

fn cal_output_size(register: &SelectorRegistry) -> (Vec<i32>, i32){
    let mut vec_of_size = Vec::new();
    let mut total_input_size : i32 = 0;
    for (k, v) in register.classes_schema.iter() {
        match k.as_str() {
            "Output" => {
                for field in v.iter() {
                    let size = match field.param_type.as_str() {
                        "i32" => {vec_of_size.push(4);4},
                        "i64" => {vec_of_size.push(4);4},
                        "f32" => {vec_of_size.push(8);8},
                        "f64" => {vec_of_size.push(8);8},
                        typ if typ.starts_with("Array<") => {

                            vec_of_size.push(4);
                            4
                        },
                        typ if typ.eq("string") => {vec_of_size.push(4);4},
                        _ => {
                            vec_of_size.push(4);
                            4
                        }
                    };
                    total_input_size += size;
                }
                println!("{} total input size: {}",k.as_str(),total_input_size);
            }
            _ => continue
        }
    }
    (vec_of_size,total_input_size)
}

fn read_utf16_string(memory: &Memory, store: &mut Store<()>, ptr: usize, max_len: usize) -> anyhow::Result<String> {
    // Read 2 * max_len bytes since UTF-16 uses 2 bytes per character
    let mut raw_bytes = vec![0u8; max_len * 2];
    memory.read(store, ptr, &mut raw_bytes)?;

    // Convert raw bytes into u16s (little endian)
    let utf16_units: Vec<u16> = raw_bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        // stop at null terminator
        .take_while(|&u| u != 0)
        .collect();

    let decoded = String::from_utf16(&utf16_units)?;
    Ok(decoded)
}


async fn wasmtime_runner(bin: &[u8], register : SelectorRegistry) -> anyhow::Result<Vec<u8>> {
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());
    let module = Module::from_binary(&engine, bin)?;
    let mut externlts : Vec<Extern> = Vec::new();
    if module.imports().len() != 0 {
        for import in module.imports() {
            println!(
                "Import module: {:#?}, name: {:#?}",
                import.module(),
                import.name(),
            );
            match import.ty() {
                ExternType::Func(func_ty) => {
                    let func = Func::new(&mut store, func_ty.clone(), |caller, params, results| {
                        if params.len() > 0 {
                            for i in params {
                                println!("param is for: {:#?}", i);
                            }
                        }
                        if results.len() > 0 {
                            for i in results {
                                println!("res {i:#?}")
                            }
                        }
                        else {
                            // results[0] = Val::I32(0);
                        }
                        Ok(())
                    });
                    externlts.push(func.into());
                }
                ExternType::Global(global_ty) => {
                    let val = match global_ty.content() {
                        ValType::I32 => Val::I32(0),
                        ValType::I64 => Val::I64(0),
                        ValType::F32 => Val::F32(0),
                        ValType::F64 => Val::F64(0),
                        _ => unimplemented!("Unsupported for dummy global type"),
                    };
                }
                ExternType::Table(table_ty) => {
                    let table = Table::new(&mut store, table_ty, Ref::Func(None))?;
                    externlts.push(table.into());
                }
                _ => unimplemented!("Don't care bruv"),
            }
        }
    }

    let instance = Instance::new(&mut store, &module, &externlts)?;
    let memory = instance.get_memory(&mut store, "memory").expect("Memory not found");
    let process_func = instance.get_typed_func::<(i32,i32),i32>(&mut store, "process")?;
    let url_global = instance.get_global(&mut store, "url").unwrap();



    let url_value = url_global.get(&mut store).i32().unwrap();
    println!("Value of url global: {}", url_value);

    let memory_data = memory.data(&store);
    let mut chars = Vec::new();
    let mut i = url_value as usize;
    while i + 1 < memory_data.len() {
        let low_byte = memory_data[i];
        let high_byte = memory_data[i + 1];
        if low_byte == 0 && high_byte == 0 {
            break;
        }
        let char_code = u16::from_le_bytes([low_byte, high_byte]);
        chars.push(char::from_u32(char_code as u32).unwrap_or('?'));
        i += 2;
    }
    let api_url = chars.into_iter().collect::<String>();
    println!("API_URL: {}", api_url);

    let mut params = HashMap::new();
    params.insert("category", "finance");
    params.insert("https", "true");

    let json = crate::https::fetch_with_query(&api_url, Some(&params), None).await?;
    println!("{}", json);

    // // Without params
    // let json2 = crate::server::server::fetch_with_query(&api_url, None, None).await.unwrap();
    // println!("Without params:\n{}", json2);

    let json_bytes = json.as_bytes();
    let json_offset = 0;

    memory.write(&mut store, json_offset as usize, json_bytes)?;

    let mut confirm = [0u8;10240];
    memory.read(&mut store, json_offset as usize, &mut confirm)?;

    let ret_ptr = process_func.call(&mut store, (json_offset, json_bytes.len() as i32))?;
    let output_offset = ret_ptr as usize;

    let mut bytes_result: Vec<u8> = Vec::new();

    if register.classes_schema.contains_key("Output") {
        let mut offset_val = 0;
        match  register.classes_schema.get("Output") {
            None => {}
            Some(val) => {
                for params in val {
                    println!("Val {:?}", params);
                    println!("Offset val: {}", offset_val);
                    match params.param_type.as_str() {
                        "i32" => {
                            let mut result = [0u8;4];
                            memory.read(&mut store, output_offset + offset_val, &mut result)?;
                            offset_val = offset_val + 4;
                            let ptr = i32::from_le_bytes(result);
                            println!("process returned pointer: {}", ptr);
                            println!("process returned pointer: {:?}", ptr.encode());
                            bytes_result.extend_from_slice(&ptr.encode());
                        },
                        "f32" => {
                            let mut result = [0u8;4];
                            memory.read(&mut store, output_offset + offset_val, &mut result)?;
                            offset_val = offset_val + 4;
                            let ptr = f32::from_le_bytes(result);
                            println!("process returned pointer: {}", ptr);
                            println!("process returned pointer: {:?}", ptr.encode());
                            bytes_result.extend_from_slice(&ptr.encode());
                        },
                        "i64" => {
                            let mut result = [0u8;8];
                            memory.read(&mut store, output_offset + offset_val, &mut result)?;
                            offset_val = offset_val + 8;
                            let ptr = i64::from_le_bytes(result);
                            println!("process returned pointer: {}", ptr);
                            println!("process returned pointer: {:?}", ptr.encode());
                            bytes_result.extend_from_slice(&ptr.encode());
                        },
                        "f64" => {
                            let mut result = [0u8;8];
                            memory.read(&mut store, output_offset + offset_val, &mut result)?;
                            offset_val = offset_val + 8;
                            let ptr = f64::from_le_bytes(result);
                            println!("process returned pointer: {}", ptr);
                            println!("process returned pointer: {:?}", ptr.encode());
                            bytes_result.extend_from_slice(&ptr.encode());
                        },
                        typ if typ.starts_with("Array<") => {
                            if let Some(inner) = typ.strip_prefix("Array<").and_then(|s| s.strip_suffix('>')) {
                                if register.classes_schema.contains_key(inner) {
                                    println!("Array of class {}", inner);
                                    let mut result = [0u8; 4];
                                    memory.read(&mut store, output_offset + offset_val, &mut result)?;
                                    offset_val = offset_val + 4;
                                    let custom_arr_ptr = u32::from_le_bytes(result) as usize;
                                    // println!("process returned pointer: {}", custom_arr_ptr);
                                    let mut array_header_bytes = [0u8; 16];
                                    memory.read(&store, custom_arr_ptr as usize, &mut array_header_bytes)?;
                                    // println!("array_header_bytes: {:?}", array_header_bytes);
                                    let pass_res_data_ptr = u32::from_le_bytes(array_header_bytes[4..8].try_into().unwrap()) as usize;
                                    let pass_res_data_cap = u32::from_le_bytes(array_header_bytes[12..16].try_into().unwrap()) as usize;
                                    if pass_res_data_ptr == 0 {
                                        panic!("pass_res data pointer is null (0)");
                                    }

                                    let mut crypto_values = Vec::new();
                                    for i in 0..pass_res_data_cap {
                                        let elem_offset = pass_res_data_ptr + (i * 4);
                                        let mut elem_bytes = [0u8; 4];
                                        memory.read(&store, elem_offset, &mut elem_bytes)?;
                                        let ptr = u32::from_le_bytes(elem_bytes) as usize;

                                        if ptr == 0 {
                                            continue;
                                        }
                                        let mut usd_bytes = [0u8; 4];
                                        memory.read(&store, ptr, &mut usd_bytes)?;
                                        let value = f32::from_le_bytes(usd_bytes);

                                        println!("CryptoValue[{}].usd: {}", i, value);
                                        println!("process returned pointer: {:?}", value.encode());
                                        crypto_values.push(value);
                                    }
                                    bytes_result.extend_from_slice(&crypto_values.encode());
                                } else {
                                    println!("Not an Array<T> type.");
                                }
                            }
                        },
                        typ if typ.eq("string") => {
                            let mut result = [0u8;4];
                            memory.read(&mut store, output_offset + offset_val, &mut result)?;
                            offset_val = offset_val + 4;
                            let ptr = u32::from_le_bytes(result) as usize;
                            let string_out = read_utf16_string(&memory,&mut store, ptr,1024).ok().unwrap_or_default();
                            println!("string out {:?}", string_out.clone());
                            println!("string out {:?}", string_out.clone().encode());
                            bytes_result.extend_from_slice(&string_out.encode());
                        }
                        _ => {
                            let mut result = [0u8;4];
                            memory.read(&mut store, output_offset + offset_val, &mut result)?;
                            offset_val = offset_val + 4;
                            let ptr = u32::from_le_bytes(result) as usize;
                            println!("process returned pointer: {}", ptr);
                        }
                    }
                }
            }

        }
    }
    println!("Byte res {:?}",bytes_result);
    // build_result(&bytes_result);
    Ok(bytes_result)
}


// Test decodable
#[derive(Debug, Decode)]
struct CryptoValue {
    usd: f32,
}

#[derive(Debug, Decode)]
struct Output {
    greater: String,
    custom: Vec<CryptoValue>,
    primi_i: i32,
    pmimi_f: f32,
}
fn build_result(val: &[u8]) {
    let bytes: Vec<u8> = val.iter().map(|x| (x & 0xFF) as u8).collect();
    match Output::decode(&mut &bytes[..]) {
        Ok(decoded) => println!("{:#?}", decoded),
        Err(e) => println!("Failed to decode: {}", e),
    }
}



