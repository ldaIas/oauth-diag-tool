-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ServerFormTests exposing (suite)

import Expect
import ServerForm exposing (init, recomputeUrls)
import Test exposing (Test, describe, test)


suite : Test
suite =
    describe "ServerForm"
        [ describe "init"
            [ test "derives all URLs from the port" <|
                \_ ->
                    init 9500
                        |> Expect.equal
                            { name = ""
                            , portInput = "9500"
                            , port_ = 9500
                            , issuerUrl = "http://localhost:9500"
                            , authorizationUrl = "http://localhost:9500/authorize"
                            , tokenEndpoint = "http://localhost:9500/token"
                            }
            ]
        , describe "recomputeUrls"
            [ test "rebuilds the URLs after the port changes" <|
                \_ ->
                    let
                        model =
                            init 9500
                    in
                    recomputeUrls { model | port_ = 8080 }
                        |> Expect.equal
                            { model
                                | port_ = 8080
                                , issuerUrl = "http://localhost:8080"
                                , authorizationUrl = "http://localhost:8080/authorize"
                                , tokenEndpoint = "http://localhost:8080/token"
                            }
            ]
        ]
