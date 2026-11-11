-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ServerListTests exposing (suite)

import Expect
import Json.Decode as Decode
import ServerList exposing (OAuthServer, findAvailable, init, nextPort, resourceAccessDecoder, serverConfigDecoder)
import Test exposing (Test, describe, test)


serverOnPort : Int -> OAuthServer
serverOnPort port_ =
    { id = port_
    , name = "server " ++ String.fromInt port_
    , issuerUrl = "http://localhost:" ++ String.fromInt port_
    , authorizationUrl = "http://localhost:" ++ String.fromInt port_ ++ "/authorize"
    , tokenEndpoint = "http://localhost:" ++ String.fromInt port_ ++ "/token"
    , port_ = port_
    , running = False
    , clients = []
    , redirectUrlOverride = ""
    , accessTokenExpiry = 3600
    , refreshTokenExpiry = 86400
    , lastResourceAccess = Nothing
    }


withServers : List Int -> ServerList.Model
withServers ports =
    { init | servers = List.map serverOnPort ports }


{-| Mirrors the JSON produced by the Rust side's `auth_server::get_all`
(serde camelCase). Changing either side should break this fixture.
-}
serverConfigJson : String
serverConfigJson =
    """
    { "id": 1
    , "configName": "my server"
    , "authServerUrl": "http://localhost:9500"
    , "tokenUrl": "http://localhost:9500/token"
    , "issuerUrl": "http://localhost:9500"
    , "port": 9500
    , "running": false
    , "clients": [ { "id": 11, "clientId": "abc", "clientSecret": "xyz" } ]
    , "redirectUrlOverride": ""
    , "accessTokenExpiry": 3600
    , "refreshTokenExpiry": 86400
    }
    """


suite : Test
suite =
    describe "ServerList"
        [ describe "nextPort"
            [ test "starts at 9500 with no servers" <|
                \_ ->
                    nextPort init
                        |> Expect.equal 9500
            , test "skips used ports" <|
                \_ ->
                    nextPort (withServers [ 9500, 9501 ])
                        |> Expect.equal 9502
            , test "fills gaps between used ports" <|
                \_ ->
                    nextPort (withServers [ 9500, 9502 ])
                        |> Expect.equal 9501
            , test "ignores unrelated high ports" <|
                \_ ->
                    nextPort (withServers [ 9600 ])
                        |> Expect.equal 9500
            ]
        , describe "findAvailable"
            [ test "returns the candidate when unused" <|
                \_ ->
                    findAvailable 9500 [ 9600 ]
                        |> Expect.equal 9500
            , test "walks past a consecutive run of used ports" <|
                \_ ->
                    findAvailable 9500 [ 9500, 9501, 9502 ]
                        |> Expect.equal 9503
            ]
        , describe "serverConfigDecoder"
            [ test "decodes the backend's camelCase payload" <|
                \_ ->
                    Decode.decodeString serverConfigDecoder serverConfigJson
                        |> Expect.equal
                            (Ok
                                { id = 1
                                , name = "my server"
                                , issuerUrl = "http://localhost:9500"
                                , authorizationUrl = "http://localhost:9500"
                                , tokenEndpoint = "http://localhost:9500/token"
                                , port_ = 9500
                                , running = False
                                , clients = [ { id = 11, clientId = "abc", clientSecret = "xyz" } ]
                                , redirectUrlOverride = ""
                                , accessTokenExpiry = 3600
                                , refreshTokenExpiry = 86400
                                , lastResourceAccess = Nothing
                                }
                            )
            , test "fails when a required field is missing" <|
                \_ ->
                    Decode.decodeString serverConfigDecoder "{\"id\":1}"
                        |> Expect.err
            ]
        , describe "resourceAccessDecoder"
            [ test "decodes a successful access (null error)" <|
                \_ ->
                    Decode.decodeString resourceAccessDecoder
                        "{\"serverId\":1,\"clientId\":\"abc\",\"status\":200,\"error\":null,\"timestamp\":\"2026-07-08T12:00:00Z\"}"
                        |> Expect.equal
                            (Ok
                                { serverId = 1
                                , clientId = "abc"
                                , status = 200
                                , error = Nothing
                                , timestamp = "2026-07-08T12:00:00Z"
                                }
                            )
            , test "decodes a failed access with an error message" <|
                \_ ->
                    Decode.decodeString resourceAccessDecoder
                        "{\"serverId\":1,\"clientId\":\"abc\",\"status\":401,\"error\":\"expired\",\"timestamp\":\"t\"}"
                        |> Result.map .error
                        |> Expect.equal (Ok (Just "expired"))
            ]
        ]
