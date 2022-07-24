use crate::{hashmap, syntax::*};
use lazy_static::lazy_static;
use std::{collections::HashMap, fmt::Display};
use thiserror::Error;

// TODO: Check that variables are only used once

lazy_static! {
    pub static ref BUILTIN_MAP: HashMap<Builtin, MachineType> = hashmap!(<Builtin, MachineType> [
        Builtin::Add => {   // (Num, Num) -> Num
            MachineType {
                var_count: 0,
                input: Type::Tuple(vec![Type::Num, Type::Num]),
                output: Type::Num
            }
        },
        Builtin::Mul => {   // (Num, Num) -> Num
            MachineType {
                var_count: 0,
                input: Type::Tuple(vec![Type::Num, Type::Num]),
                output: Type::Num
            }
        },
        Builtin::Mod => {   // (Num, Num) -> Num
            MachineType {
                var_count: 0,
                input: Type::Tuple(vec![Type::Num, Type::Num]),
                output: Type::Num
            }
        },
        Builtin::Pow => {   // (Num, Num) -> Num
            MachineType {
                var_count: 0,
                input: Type::Tuple(vec![Type::Num, Type::Num]),
                output: Type::Num
            }
        },
        Builtin::Sqrt => {   // Num -> Num
            MachineType {
                var_count: 0,
                input: Type::Tuple(vec![Type::Num]),
                output: Type::Num
            }
        },
        Builtin::Gte => {   // (Num, Num) -> Bool
            MachineType {
                var_count: 0,
                input: Type::Tuple(vec![Type::Num, Type::Num]),
                output: Type::Bool
            }
        },
        Builtin::Lt => {   // (Num, Num) -> Bool
            MachineType {
                var_count: 0,
                input: Type::Tuple(vec![Type::Num, Type::Num]),
                output: Type::Bool
            }
        },
        Builtin::Eq => {   // forall a. (a, a) -> Bool
            MachineType {
                var_count: 1,
                input: Type::Tuple(vec![Type::TyVar(0), Type::TyVar(0)]),
                output: Type::Bool
            }
        },
        Builtin::Dup2 => {  // forall a. a -> (a, a)
            MachineType {
                var_count: 1,
                input: Type::TyVar(0),
                output: Type::Tuple(vec![Type::TyVar(0), Type::TyVar(0)])
            }
        },
        Builtin::Dup3 => {  // forall a. a -> (a, a, a)
            MachineType {
                var_count: 1,
                input: Type::TyVar(0),
                output: Type::Tuple(vec![Type::TyVar(0), Type::TyVar(0), Type::TyVar(0)])
            }
        },
        Builtin::Print => { // forall a. a -> a
            MachineType {
                var_count: 1,
                input: Type::TyVar(0),
                output: Type::TyVar(0)
            }
        }
    ]);
}

#[derive(Debug, Clone)]
pub enum Type {
    Num,
    Bool,
    String,
    Tuple(Vec<Type>),
    TyVar(usize),
    UnifVar(usize),
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Num => "num".to_owned(),
            Self::Bool => "bool".to_owned(),
            Self::String => "string".to_owned(),
            Self::Tuple(value) => format!(
                "{})",
                value
                    .iter()
                    .fold("(".to_owned(), |acc, ty| format!("{acc} {ty}"))
            ),
            Self::TyVar(value) => format!("Type Var: {value}"),
            Self::UnifVar(value) => format!("Uniform Var: {value}"),
        };

        write!(f, "{message}")
    }
}

#[derive(Debug, Clone, Error)]
pub enum TypeError {
    #[error("cannot unify {0} with {1}")]
    CannotUnify(Type, Type),
}

#[derive(Debug, Clone)]
pub struct MachineType {
    var_count: usize,
    input: Type,
    output: Type,
}

struct GlobalTypeEnv {
    machine_types: HashMap<String, MachineType>,
}

struct LocalTypeEnv {
    var_types: HashMap<String, Type>,
    unification_constraints: Vec<(Type, Type)>,
    last_unification_var: usize,
}

pub fn check(program: &Program) -> Result<(), TypeError> {
    let mut global_env = GlobalTypeEnv {
        machine_types: HashMap::new(),
    };
    for machine in &program.machines {
        check_machine_def(&mut global_env, machine)?;
    }
    Ok(())
}

fn check_machine_def(
    global_env: &mut GlobalTypeEnv,
    machine: &Definition,
) -> Result<(), TypeError> {
    let mut local_env = LocalTypeEnv {
        var_types: HashMap::new(),
        unification_constraints: Vec::new(),
        last_unification_var: 0,
    };
    // The type of the machine itself is unknown right now, but we need it to check recursive calls
    // so we construct a machine type out of unification variables
    let machine_type_input = new_unif_var(&mut local_env);
    let machine_type_output = new_unif_var(&mut local_env);
    let machine_type = MachineType {
        var_count: 0,
        input: machine_type_input,
        output: machine_type_output.clone(),
    };
    global_env
        .machine_types
        .insert(machine.name.clone(), machine_type.clone());

    for statement in &machine.body {
        check_statement(global_env, &mut local_env, statement);
    }

    let real_output_type = infer_stream(global_env, &mut local_env, &machine.result);
    local_env
        .unification_constraints
        .push((machine_type_output, real_output_type));

    let subst = unify(&local_env)?;

    let generalized_machine_type = generalize(&subst, machine_type);
    global_env
        .machine_types
        .insert(machine.name.clone(), generalized_machine_type);

    Ok(())
}

fn check_statement(
    global_env: &mut GlobalTypeEnv,
    local_env: &mut LocalTypeEnv,
    statement: &Statement,
) {
    match statement {
        Statement::Consume(stream) => {
            let _ = infer_stream(global_env, local_env, stream);
        }
        Statement::Let(vars, stream) => {
            let stream_ty = infer_stream(global_env, local_env, stream);

            if vars.len() == 1 {
                // If we only bind a single variable, there is no destructuring involved
                // so we don't need to check against anything
                local_env.var_types.insert(vars[0].clone(), stream_ty);
            } else {
                // Type checking detructuring is achieved by generating a Tuple type made up of
                // unification variables and checking that against the inferred stream type.
                // These unification variables are the types of the corresponding local variables
                let tuple_tys: Vec<_> = vars.iter().map(|x| (x, new_unif_var(local_env))).collect();

                for (x, ty) in tuple_tys.iter() {
                    local_env.var_types.insert(String::from(*x), ty.clone());
                }

                let variable_tuple_ty =
                    Type::Tuple(tuple_tys.into_iter().map(|(_, ty)| ty).collect());
                local_env
                    .unification_constraints
                    .push((variable_tuple_ty, stream_ty))
            }
        }
    }
}

fn infer_stream(
    global_env: &mut GlobalTypeEnv,
    local_env: &mut LocalTypeEnv,
    stream: &Stream,
) -> Type {
    match stream {
        Stream::Var(name) => match local_env.var_types.get(name) {
            Some(ty) => ty.clone(),
            None => panic!(
                "infer_stream: Unbound variable found during type checking: '{}'",
                name
            ),
        },
        Stream::Const(Value::Null) => {
            // 'null' can have any type, so we treat it like 'forall a. a'
            new_unif_var(local_env)
        }
        Stream::Const(Value::Num(_)) => Type::Num,
        Stream::Const(Value::Str(_)) => Type::String,
        Stream::Const(Value::Bool(_)) => Type::Bool,
        Stream::Const(Value::Tuple(_)) => {
            panic!("infer_stream: Tuple constants should not be able to appear in source files")
        }

        Stream::Pipe(stream, machine) => {
            let stream_ty = infer_stream(global_env, local_env, stream);

            let machine_ty = match &**machine {
                Machine::Var(machine_name) => match global_env.machine_types.get(machine_name) {
                    Some(ty) => ty.clone(),
                    None => panic!(
                        "infer_stream: Unbound machine found during type checking: '{}'",
                        machine_name
                    ),
                },
                Machine::Builtin(builtin) => get_builtin_ty(builtin)
                    .unwrap_or_else(|| panic!("{builtin:#?} not found in BUILTIN_MAP")),
                Machine::Defined(_, _) => panic!(
                    "infer_stream: Machine::Defined should not be able to appear in source files"
                ),
            };
            let machine_ty = instantiate(local_env, machine_ty);

            local_env
                .unification_constraints
                .push((machine_ty.input.clone(), stream_ty));
            machine_ty.output
        }

        Stream::Zip(streams) => {
            let stream_tys = streams
                .iter()
                .map(|stream| infer_stream(global_env, local_env, stream))
                .collect();
            Type::Tuple(stream_tys)
        }

        Stream::Cond(condition, then, else_) => {
            let condition_ty = infer_stream(global_env, local_env, condition);
            local_env
                .unification_constraints
                .push((condition_ty, Type::Bool));

            let then_ty = infer_stream(global_env, local_env, then);
            let else_ty = infer_stream(global_env, local_env, else_);

            local_env
                .unification_constraints
                .push((then_ty.clone(), else_ty));

            // Since we made sure the types of the 'then' and 'else' expressions are
            // equivalent, it doesn't matter which one we return here. We arbitrarily pick the 'then' branch.
            then_ty
        }
        Stream::Limit(stream, _) => infer_stream(global_env, local_env, stream),

        Stream::Unzip(_, _) => {
            panic!("infer_stream: Stream::Unzip should not be able to appear in source files")
        }
    }
}

fn get_builtin_ty(builtin: &Builtin) -> Option<MachineType> {
    BUILTIN_MAP.get(builtin).map(|ty| ty.to_owned())
}

fn new_unif_var(local_env: &mut LocalTypeEnv) -> Type {
    let var_id = local_env.last_unification_var;
    local_env.last_unification_var += 1;
    Type::UnifVar(var_id)
}

fn instantiate(local_env: &mut LocalTypeEnv, machine_ty: MachineType) -> MachineType {
    let unif_vars: Vec<_> = (0..machine_ty.var_count)
        .map(|i| (i, new_unif_var(local_env)))
        .collect();

    let input = unif_vars
        .iter()
        .rfold(machine_ty.input, |ty, (i, var)| replace_ty_var(ty, *i, var));
    let output = unif_vars.iter().rfold(machine_ty.output, |ty, (i, var)| {
        replace_ty_var(ty, *i, var)
    });

    MachineType {
        var_count: 0,
        input,
        output,
    }
}

fn replace_ty_var(ty: Type, var: usize, to_replace: &Type) -> Type {
    match ty {
        Type::Num | Type::Bool | Type::String | Type::UnifVar(_) => ty,
        Type::TyVar(other) => {
            if other == var {
                to_replace.clone()
            } else {
                ty
            }
        }
        Type::Tuple(tys) => Type::Tuple(
            tys.into_iter()
                .map(|ty| replace_ty_var(ty, var, to_replace))
                .collect(),
        ),
    }
}

fn replace_unif_var(ty: Type, var: usize, to_replace: &Type) -> Type {
    match ty {
        Type::Num | Type::Bool | Type::String | Type::TyVar(_) => ty,
        Type::UnifVar(other) => {
            if other == var {
                to_replace.clone()
            } else {
                ty
            }
        }
        Type::Tuple(tys) => Type::Tuple(
            tys.into_iter()
                .map(|ty| replace_ty_var(ty, var, to_replace))
                .collect(),
        ),
    }
}

fn unify(local_env: &LocalTypeEnv) -> Result<HashMap<usize, Type>, TypeError> {
    let mut subst: HashMap<usize, Type> = HashMap::new();

    for (ty1, ty2) in &local_env.unification_constraints {
        unify_types(&mut subst, ty1, ty2)?
    }

    Ok(subst)
}

fn unify_types(subst: &mut HashMap<usize, Type>, ty1: &Type, ty2: &Type) -> Result<(), TypeError> {
    match (ty1, ty2) {
        (Type::Num, Type::Num) | (Type::Bool, Type::Bool) | (Type::String, Type::String) => Ok(()),
        (Type::TyVar(a), Type::TyVar(b)) if a == b => Ok(()),
        (Type::Tuple(tys1), Type::Tuple(tys2)) if tys1.len() == tys2.len() => {
            for (ty1, ty2) in tys1.iter().zip(tys2.iter()) {
                unify_types(subst, ty1, ty2)?
            }
            Ok(())
        }
        (Type::UnifVar(a), ty2) => {
            let a_type = match subst.get(a) {
                Some(ty) => Some(ty.clone()),
                None => {
                    // TODO: occurs check
                    subst.insert(*a, ty2.clone());
                    None
                }
            };
            match a_type {
                Some(ty) => unify_types(subst, &ty, ty2),
                None => Ok(()),
            }
        }
        (ty1, Type::UnifVar(b)) => {
            unify_types(subst, &Type::UnifVar(*b), ty1) // Swap the types to avoid having to duplicate unif var logic
        }
        _ => Err(TypeError::CannotUnify(ty1.clone(), ty2.clone())),
    }
}

fn free_unif_vars(subst: &HashMap<usize, Type>, result: &mut Vec<usize>, ty: &Type) {
    match ty {
        Type::UnifVar(a) => match subst.get(a) {
            None => result.push(*a),
            Some(ty) => free_unif_vars(subst, result, ty),
        },
        Type::Bool | Type::Num | Type::String | Type::TyVar(_) => (),
        Type::Tuple(tys) => {
            for ty in tys {
                free_unif_vars(subst, result, ty)
            }
        }
    }
}

fn generalize(subst: &HashMap<usize, Type>, machine_ty: MachineType) -> MachineType {
    let mut free_vars = Vec::new();
    free_unif_vars(subst, &mut free_vars, &machine_ty.input);
    free_unif_vars(subst, &mut free_vars, &machine_ty.output);

    let input = free_vars
        .iter()
        .enumerate()
        .rfold(machine_ty.input, |ty, (i, var)| {
            replace_unif_var(ty, *var, &Type::TyVar(i))
        });
    let output = free_vars
        .iter()
        .enumerate()
        .rfold(machine_ty.output, |ty, (i, var)| {
            replace_unif_var(ty, *var, &Type::TyVar(i))
        });

    MachineType {
        var_count: free_vars.len(),
        input,
        output,
    }
}
