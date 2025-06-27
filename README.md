# RTK - Rust Type Kit (beta)

RTK is a tool that allows you to write scripts that query your Rust codebase for type information and emit types in another programming language.

RTK can be used for...

* Generating OpenAPI bindings for an Axum server
* Directly generating TypeScript bindings for an Axum server
* Checking types of SQL queries statically, or generating Rust types from SQL queries
* Generating a typed API for a Lua engine embedded in your app (which this repo itself does)
* ...and much more

In fact, RTK itself is used to implement its own Lua API!

## Advantages

RTK wraps rustc to perform dense and accurate analysis of your codebase. This means that types are 100% accurate and fully inferred (something proc macro solutions can't do because they have no type information).
Additionally, compared to other solutions like [utopia](https://github.com/juhaku/utoipa), you don't need to change your code or wrap it with a third party dependency. You can stay close to the original API in your source code, and
generate API bindings completely separately with your own specification and capabilities.

Additionally, RTK uses Lua to define your spec so its fairly simple to understand and fast to hack with.

### Currently queryable information

* Method calls matching a method's definition path
* Function calls matching a function's definition path
* Function definitions matching a function's definition path
* Trait impl blocks matching a trait's definition path (including the function definitions of each function in that impl)

Additionally, all _type information_ automatically gets resolved for each dependency of a queried item. For instance, querying a function will automatically recursively resolve all types required in the functions parameters and return types.
Within this type information, you can know:

* Proc macro attributes (meaning you can implement serde compatibility with your types)
* Asyncness of a return type
* All field names of a struct
* All enum variants and their data if they have any
* Most of the common Rust primitives, like all integers, strings, etc
* Most of the common Rust std types, like `Option`, `HashMap`, and `Result`

Take a look at the [examples](examples/) as well as the [Lua API](lua/rtk_api.lua) to see how to use these.

## Beta notice

This project is in its very early stages, and documentation is fairly sparse and some API is missing such as function generic parameters and some more std types.
The absolute best source for seeing capabilities is to look at the generated Lua API, which can be found [here](lua/rtk_api.lua). Contributions are welcome, and encouraged!
The codebase is fairly small and tidy, so new contributors should be able to jump in quite easily.

## Quick Start

### Installation

Install RTK through cargo:

```
cargo install rtk
```

### Project setup

First, enter your existing Rust project:

```
cd <my-project>
```

Create an RTK lua script:

```
touch <any_name_here>.lua
```

Then fill it out with some code. Take a look at the [example](examples/axum-to-ts/rtk.lua) for a good skeleton for your own project.

When writing your script, the `rtk.version` line at the top is very important as it tells the CLI which API version was used. If you're running RTK from a `cargo install`, set it to whatever version of the CLI you pulled and you'll be golden!

Using the wrong version means that our API could change and your script will break, which is why its important to set it well.

Finally, run RTK:

```
rtk --script <your_script>.lua --out-file <your_out_file>.<ext> -- <extra cargo args, like `-p <specific crate>
```

The out file specifies what file calls, in Lua, `rtk.emit` will write to.

If you'd like to run RTK on the example, assuming you have RTK installed (or you can install locally with `./scripts/install-rtk-cli`), you can run the following command from the root of this repo:

```
rtk --script examples/axum-to-ts/rtk.lua --out-file examples/axum-to-ts/api.ts -- -p axum-to-ts
```

## Demo (Axum routes -> TypeScript)

NOTE: A lot of the demo code has been omitted for README brevity, see full example [here](examples/axum-to-ts)!

Given the following basic Axum code setup:

```rust
async fn main() {
    Router::new()
        .route("/user", post(add_user))
        .route("/user/{id}", get(get_user));

    // ...serve app code
}

struct AddUserRequest { username: String }
struct User { id: u32, username: String }

async fn add_user(req: axum::extract::Json<AddUserRequest>) -> Json<User> {
    // body omitted 
}

async fn get_user(Path(id): axum::extract::Path<u32>) -> Json<User> {
    // body omitted
}
```

We can write a Lua script to query Rust for our route types.

In this snippet, the `.route` method is perfect as the entrypoint since all of our routes go through there!
To query for it, we can use the `query_method_calls` method.

```lua
-- we need to specify the version for backwards compatibility, in your app you can pin this
-- to a crates io version like such: rtk.version("0.1.0")
rtk.version("local:crates/rtk-rustc-driver")

local routes = rtk.query_method_calls({
	location = {
		crate_name = "axum",
		path = { "routing", "route" },
		impl_block_number = 3,
	},
})
```

To run that query, we need to specify the crate name the method call lives in, the full path to the method, and which impl block
the method lives in. The impl block is required since there can be multiple methods of the same name in the module.
Finding out the impl block number is very easy; you can simply omit it from your script, run your script once (explained below), and
RTK will tell you which impl blocks it found that match that path.

At this point, we now have all of our route method calls, so lets write a loop around them:

```lua
-- this is each call to `.route("/route-here", method_here(route_fn_here))`
for _, route in ipairs(routes) do
    -- subsequent code will be in here
end
```

We can pull out our route string, first:

```lua
-- this will correspond to `/user` and `/user/{id}`
local route_path_arg = route.args[1]
assert(route_path_arg.variant_name == "StringLiteral", "First argument to route must be a string literal")
```

Then we can pull out the method function call:

```lua
local route_method_call = route.args[2]
assert(
	route_method_call.variant_name == "FunctionCall",
	"Second argument to route must be a route method function call"
)

-- the method itself will be the last value in the path to the function call, i.e. the route itself
-- the full method path will be for instance axum::routing::method_routing::get, so we only want the tail
local method = route_method_call.variant_data.location.path[#route_method_call.variant_data.location.path]

-- the first (and only) argument to the route method function is the route fn, so lets grab that
local route_fn = route_method_call.variant_data.args[1]
-- this time, we're expecting a typed value itself to be passed in and not another function call, so lets pull
-- out the route fn
assert(route_fn.variant_name == "Type", "First argument to route method must be a type")
assert(route_fn.variant_data.variant_name == "Function", "First argument to route method must be a function")

local route_fn_args = route_fn.variant_data.variant_data.args_struct
local route_fn_ret_type = route_fn.variant_data.variant_data.return_type

local route_fn_name =
    route_fn.variant_data.variant_data.location.path[#route_fn.variant_data.variant_data.location.path]
```

Now with all the data we want, we can begin to make the output! The utility functions, such as `route_param_names`, `try_axum_tuple_extractor`, and
`rust_type_into_typescript_type` are defined in the example crate, so I highly suggest reading that as I want to keep this readme
fairly concise.

```lua
local param_names = route_param_names(route_path_arg.variant_data)

local ts_fn_args_str = ""
for _, arg in ipairs(route_fn_args.fields) do
	local maybe_extracts_path = try_axum_tuple_extractor(arg.value, "Path")
	if maybe_extracts_path then
		-- this is a path extractor, so we can use the param names from the route string
		local param_name = table.remove(param_names, 1)
		ts_fn_args_str = ts_fn_args_str
			.. string.format("%s: %s, ", param_name, rust_type_into_typescript_type(maybe_extracts_path))
	end

	local maybe_extracts_json = try_axum_tuple_extractor(arg.value, "Json")
	if maybe_extracts_json then
		-- this is a json extractor, so we can use the type of the json value
		ts_fn_args_str = ts_fn_args_str
			.. string.format("json: %s, ", rust_type_into_typescript_type(maybe_extracts_json))
	end

	local ts_fn_returns_str = "void"
	if route_fn_ret_type then
		rtk.note("Route function returns type: " .. route_fn_ret_type.variant_name)
		local maybe_returns_json = try_axum_tuple_extractor(route_fn_ret_type, "Json")
		if maybe_returns_json then
			ts_fn_returns_str = rust_type_into_typescript_type(maybe_returns_json)
		end
	end

	rtk.emit(string.format(
		[[

export async function %s(%s): Promise<%s> {
	return fetch("%s", {
		method: "%s",
		headers: json ? {
			"Content-Type": "application/json",
		} : {},
		body: json && JSON.stringify(json),
	});
}
]],
		route_fn_name,
		ts_fn_args_str,
		ts_fn_returns_str,
		route_path_arg.variant_data,
		method:upper()
	))
end
```

And that's it! Running this will produce the following output:

```typescript
export async function get_user(id: number, ): Promise<{ id: number, username: string }> {
	return fetch("/user/{id}", {
		method: "GET",
		headers: json ? {
			"Content-Type": "application/json",
		} : {},
		body: json && JSON.stringify(json),
	});
}

export async function add_user(json: { username: string }, ): Promise<{ id: number, username: string }> {
	return fetch("/user", {
		method: "POST",
		headers: json ? {
			"Content-Type": "application/json",
		} : {},
		body: json && JSON.stringify(json),
	});
}
```
