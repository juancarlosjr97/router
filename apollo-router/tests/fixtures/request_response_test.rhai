// These tests ensure that we can process request/responses and that we are receiving the
// correct values provided in the rust layer.
// If any of the tests fail, the thrown error will cause the respective rust
// unit test to fail.

fn process_common_request(check_context_method_and_id, check_body, request) {
    if check_context_method_and_id {
        if request.context.entries != () {
            throw(`context entries: expected: (), actual: ${request.context.entries}`);
        }
        if request.method != "GET" {
            throw(`query: expected: "GET", actual: ${request.method}`);
        }
        if type_of(request.id) != "string" {
            throw(`query: expected: "string", actual: ${type_of(request.id)}`);
        }
    }
    if check_body {
        if request.body.operation_name != () {
            throw(`operation name: expected: canned, actual: ${request.body.operation_name}`);
        }
        if request.body.query != () {
            throw(`query: expected: (), actual: ${request.body.query}`);
        }
        if request.body.variables != #{} {
            throw(`query: expected: #{}, actual: ${request.body.variables}`);
        }
        if request.body.extensions != #{} {
            throw(`query: expected: #{}, actual: ${request.body.extensions}`);
        }
    }
    if request.uri.host != () {
        throw(`query: expected: (), actual: ${request.uri.host}`);
    }
    if request.uri.path != "/" {
        throw(`query: expected: "/", actual: ${request.uri.path}`);
    }
    if request.uri.port != {} {
        throw(`query: expected: {}, actual: ${request.uri.port}`);
    }
}

fn process_router_request(request){
    process_common_request(true, false, request);
}

fn process_supergraph_request(request) {
    if request.context.entries != () {
        throw(`context entries: expected: (), actual: ${request.context.entries}`);
    }
    if request.headers["content-type"] != "application/json" {
        throw(`header["content-type"]: expected: "application/json", actual: ${request.headers.content-type}`);
    }
    if request.method.to_string() != "POST" {
        throw(`query: expected: "POST", actual: ${request.method}`);
    }
    if request.body.operation_name != "canned" {
        throw(`operation name: expected: canned, actual: ${request.body.operation_name}`);
    }
    let expected_query = `

            query TopProducts($first: Int) {
                topProducts(first: $first) {
                    upc
                    name
                    reviews {
                        id
                        product { name }
                        author { id name }
                    }
                }
            }
        `;
    if request.body.query != expected_query {
        throw(`query: expected: ${expected_query}, actual: ${request.body.query}`);
    }
    if request.body.variables != #{"first": 2} {
        throw(`query: expected: #{"first": 2}, actual: ${request.body.variables}`);
    }
    if request.body.extensions != #{} {
        throw(`query: expected: #{}, actual: ${request.body.extensions}`);
    }
    if request.uri.host != "default" {
        throw(`query: expected: "default", actual: ${request.uri.host}`);
    }
    if request.uri.path != "/" {
        throw(`query: expected: "/", actual: ${request.uri.path}`);
    }
}

fn process_execution_request(request) {
    process_common_request(true, true, request);
    if request.query_plan != "" {
        throw(`query: expected: (), actual: ${request.query_plan}`);
    }
}

fn process_subgraph_request(request) {
    process_common_request(true, true, request);
    // subgraph doesn't have a context member
    process_common_request(false, true, request.subgraph);

    if request.subgraph_request_id == () {
        throw(`subgraph request must have a subgraph request id`);
    }
}

fn test_response_is_primary(response) {
    if !response.is_primary() {
        throw #{
            status: 500,
            message: "should be primary"
        };
    }
}

fn test_response_is_not_primary(response) {
    if response.is_primary() {
        throw #{
            status: 500,
            message: "should not be primary"
        };
    }
}

fn test_response_body(response) {
    if response.body.label != () {
        throw(`label: expected: (), actual: ${response.body.label}`);
    }
    if response.body.data != () {
        throw(`query: expected: (), actual: ${response.body.data}`);
    }
    if response.body.errors != [] {
        throw(`query: expected: [], actual: ${response.body.errors}`);
    }
    if response.body.extensions != #{} {
        throw(`query: expected: #{}, actual: ${response.body.extensions}`);
    }
}

fn test_response_status_code(response) {
    if type_of(response.status_code) != "http::status::StatusCode" {
        throw(`status_code: expected: "http::status::StatusCode", actual: ${type_of(response.status_code)}`);
    }
    if parse_int(response.status_code.to_string()) != 200 {
        throw(`status_code: expected: 200, actual: ${response.status_code}`);
    }
    if response.status_code != status_code_from_int(200) {
        throw(`status_code: expected: 200, actual: ${response.status_code}`);
    }
}

fn process_common_response(response) {
    if response.context.entries != () {
        throw(`context entries: expected: (), actual: ${response.context.entries}`);
    }
    if type_of(response.id) != "string" {
        throw(`query: expected: "string", actual: ${type_of(response.id)}`);
    }
}

fn test_parse_request_details(request){
    if request.uri.host != "not-default" {
        throw(`query: expected: not-default, actual: ${request.uri.host}`);
    }
    if request.uri.path != "/path" {
        throw(`query: expected: "/path", actual: ${request.uri.path}`);
    }
    if request.uri.port != 8080 {
        throw(`query: expected: 8080, actual: ${request.uri.port}`);
    }
}

fn process_router_response(response) {
    test_response_is_primary(response);
    process_common_response(response);
    test_response_status_code(response);
}

fn process_router_chunked_response(response) {
    test_response_is_not_primary(response);
    process_common_response(response);
}

fn process_supergraph_response(response) {
    test_response_is_primary(response);
    process_common_response(response);
    test_response_body(response);
}

fn process_supergraph_deferred_response(response) {
    test_response_is_not_primary(response);
    process_common_response(response);
    test_response_body(response);
}

fn process_execution_response(response) {
    test_response_is_primary(response);
    process_common_response(response);
    test_response_body(response);
}

fn process_execution_deferred_response(response) {
    test_response_is_not_primary(response);
    process_common_response(response);
    test_response_body(response);
}

fn process_subgraph_response(response) {
    process_common_response(response);
    test_response_body(response);
    test_response_status_code(response);
     if response.subgraph_request_id == () {
        throw(`subgraph response must have a subgraph request id`);
    }
}

fn process_subgraph_response_om_forbidden(response) {
    throw #{
        status: 403,
        message: "I have raised a 403"
    };
}

fn process_subgraph_response_om_forbidden_graphql(response) {
    throw #{
        status: 403,
        body: #{
            errors: [#{
                message: `I have raised a 403`,
                extensions: #{
                    code: "ACCESS_DENIED"
                }
            }]
        }
    };
}


fn process_subgraph_response_om_200_graphql(response) {
    throw #{
        status: 200,
        body: #{
            data: #{
                name: "Ada Lovelace"
            }
        }
    };
}
fn process_subgraph_response_string(response) {
    throw "I have raised an error";
}

fn process_subgraph_response_om_ok(response) {
    throw #{
        status: 200,
        message: "I have raised a 200"
    };
}

fn process_subgraph_response_om_missing_message(response) {
    throw #{
        status: 400,
    };
}