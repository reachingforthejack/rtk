# RTK - Rust Type Kit (Beta)

RTK is a tool for querying Rust codebases to extract accurate type information and emit types in other programming languages.

## Use Cases

- Generate OpenAPI or TypeScript bindings directly from Axum server code.
- Statically check SQL query types or generate Rust types based on SQL schemas.
- Create typed APIs for Lua engines embedded within Rust applications.

RTK itself uses this mechanism for its Lua API.

## Advantages

- Accurate and complete type inference via rustc.
- No modification of source code, macros or dependencies required.
- Emission scripts are written in Lua full control targeted specifically to your project.

## Query Capabilities

Currently, RTK supports querying:

- Method calls by definition path.
- Function calls and definitions by path.
- Trait implementations and associated functions.

RTK resolves all required type dependencies recursively, including:

- Proc macro attributes (e.g. `serde` annotations).
- Async return types.
- Struct field names.
- Enum variants and associated data.
- Common Rust primitives and standard library types (`Option`, `HashMap`, `Result`, etc.).

Examples and the Lua API can be found in the [`examples`](examples/) directory and the [`Lua API definition file`](lua/rtk_api.lua).

## Beta Notice

RTK is in early development. Documentation is limited, and certain features such as generic parameters and extended standard types may be incomplete. Review the generated Lua API in [`lua/rtk_api.lua`](lua/rtk_api.lua) for the most accurate reference.

Contributions are encouraged; the codebase is fairly slim and approachable for new contributors.

## Quick Start

### Installation

```sh
cargo install rtk
```

### Project Setup

Navigate to your Rust project and create a Lua script:

```sh
cd <my-project>
touch <script_name>.lua
```

Use the [`examples/axum-to-ts/rtk.lua`](examples/axum-to-ts/rtk.lua) file as a reference template. Ensure the Lua script specifies the RTK API version:

```lua
rtk.version("0.1.0")
```

Run RTK:

```sh
rtk --script <script_name>.lua --out-file <output_file> -- -p <crate_name>
```

RTK writes emitted results to the specified output file.

## Axum Example

Given this Axum setup:

```rust
async fn main() {
    Router::new()
        .route("/user", post(add_user))
        .route("/user/{id}", get(get_user));
}

struct AddUserRequest { username: String }
struct User { id: u32, username: String }

async fn add_user(Json(req): Json<AddUserRequest>) -> Json<User> {}
async fn get_user(Path(id): Path<u32>) -> Json<User> {}
```

The corresponding RTK Lua script to generate TypeScript bindings:

```lua
rtk.version("local:crates/rtk-rustc-driver")

local routes = rtk.query_method_calls({
	location = {
		crate_name = "axum",
		path = {"routing", "route"},
		impl_block_number = 3,
	},
})

for _, route in ipairs(routes) do
	local path_arg = route.args[1]
	local method_call = route.args[2]
	local method = method_call.variant_data.location.path[#method_call.variant_data.location.path]
	local route_fn = method_call.variant_data.args[1]

	local args = route_fn.variant_data.variant_data.args_struct
	local ret_type = route_fn.variant_data.variant_data.return_type
	local fn_name = route_fn.variant_data.variant_data.location.path[#route_fn.variant_data.variant_data.location.path]

	-- Utility functions like `route_param_names`, `try_axum_tuple_extractor`,
	-- and `rust_type_into_typescript_type` are defined in examples.

	rtk.emit(string.format(
		[[
export async function %s(params): Promise<ReturnType> {
	return fetch("%s", { method: "%s" });
}
]],
		fn_name,
		path_arg.variant_data,
		method:upper()
	))
end
```

This generates:

```typescript
export async function get_user(id: number): Promise<{ id: number, username: string }> {
	return fetch("/user/{id}", { method: "GET" });
}

export async function add_user(json: { username: string }): Promise<{ id: number, username: string }> {
	return fetch("/user", { method: "POST" });
}
```

Refer to [`examples/axum-to-ts`](examples/axum-to-ts) for the complete demonstration.

