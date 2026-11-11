-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ClientConfigListTests exposing (suite)

import ClientConfigList
    exposing
        ( AuthOutcome(..)
        , ClientConfig
        , authResultDecoder
        , clientConfigDecoder
        , encodeConfigFields
        , jsonStringDict
        , metadataResultDecoder
        )
import Dict
import Expect
import Json.Decode as Decode
import Json.Encode as Encode
import Test exposing (Test, describe, test)


sampleConfig : ClientConfig
sampleConfig =
    { id = 7
    , name = "my client"
    , issuerUrl = "http://localhost:9500"
    , authorizationUrl = "http://localhost:9500/authorize"
    , tokenUrl = "http://localhost:9500/token"
    , clientId = "client-abc"
    , clientSecret = "secret-xyz"
    , scopes = "openid offline_access"
    , grantType = "authorization_code"
    , extraParams = Dict.fromList [ ( "audience", "api" ), ( "prompt", "consent" ) ]
    , disabledParams = Dict.fromList [ ( "scope", True ), ( "state", False ) ]
    , disabledTokenParams = Dict.fromList [ ( "client_secret", True ) ]
    , scopesSupported = "openid profile"
    , refreshToken = "rt-12345"
    , disabledRefreshParams = Dict.fromList [ ( "scope", True ) ]
    }


{-| encodeConfigFields deliberately omits id and refreshToken (they are not
part of the create/update payload), so supply them explicitly to feed the
decoder a complete config object.
-}
encodeFullConfig : ClientConfig -> Encode.Value
encodeFullConfig config =
    Encode.object
        (( "id", Encode.int config.id )
            :: ( "refreshToken", Encode.string config.refreshToken )
            :: encodeConfigFields config
        )


suite : Test
suite =
    describe "ClientConfigList"
        [ describe "clientConfigDecoder / encodeConfigFields"
            [ test "round-trips a full config" <|
                \_ ->
                    Decode.decodeValue clientConfigDecoder (encodeFullConfig sampleConfig)
                        |> Expect.equal (Ok sampleConfig)
            , test "round-trips empty extra params as the empty string" <|
                \_ ->
                    let
                        config =
                            { sampleConfig | extraParams = Dict.empty }
                    in
                    Decode.decodeValue clientConfigDecoder (encodeFullConfig config)
                        |> Expect.equal (Ok config)
            ]
        , describe "jsonStringDict"
            [ test "decodes the empty string to an empty dict" <|
                \_ ->
                    Decode.decodeValue (jsonStringDict Decode.bool) (Encode.string "")
                        |> Expect.equal (Ok Dict.empty)
            , test "decodes a JSON object string to a dict" <|
                \_ ->
                    Decode.decodeValue (jsonStringDict Decode.bool) (Encode.string "{\"a\":true,\"b\":false}")
                        |> Expect.equal (Ok (Dict.fromList [ ( "a", True ), ( "b", False ) ]))
            , test "falls back to an empty dict on malformed JSON" <|
                \_ ->
                    Decode.decodeValue (jsonStringDict Decode.string) (Encode.string "{not json")
                        |> Expect.equal (Ok Dict.empty)
            , test "fails when the field is not a string" <|
                \_ ->
                    Decode.decodeValue (jsonStringDict Decode.bool) (Encode.int 5)
                        |> Expect.err
            ]
        , describe "authResultDecoder"
            [ test "decodes the error arm" <|
                \_ ->
                    Decode.decodeString authResultDecoder "{\"configId\":1,\"error\":\"boom\"}"
                        |> Expect.equal (Ok { configId = 1, outcome = AuthFailure "boom" })
            , test "decodes the success arm" <|
                \_ ->
                    Decode.decodeString authResultDecoder
                        "{\"configId\":2,\"statusCode\":200,\"headers\":[[\"content-type\",\"application/json\"]],\"body\":\"{}\"}"
                        |> Expect.equal
                            (Ok
                                { configId = 2
                                , outcome =
                                    AuthSuccess
                                        { statusCode = 200
                                        , headers = [ ( "content-type", "application/json" ) ]
                                        , body = "{}"
                                        }
                                }
                            )
            , test "error wins when both error and success fields are present" <|
                \_ ->
                    Decode.decodeString authResultDecoder
                        "{\"configId\":3,\"error\":\"boom\",\"statusCode\":200,\"headers\":[],\"body\":\"\"}"
                        |> Expect.equal (Ok { configId = 3, outcome = AuthFailure "boom" })
            ]
        , describe "metadataResultDecoder"
            [ test "decodes a form-submission fetch (null configId) error" <|
                \_ ->
                    Decode.decodeString metadataResultDecoder "{\"configId\":null,\"error\":\"unreachable\"}"
                        |> Expect.equal (Ok { configId = Nothing, outcome = Err "unreachable" })
            , test "decodes the success arm" <|
                \_ ->
                    Decode.decodeString metadataResultDecoder
                        "{\"configId\":4,\"body\":\"{}\",\"authorizationEndpoint\":\"http://localhost:9500/authorize\",\"tokenEndpoint\":\"http://localhost:9500/token\",\"scopesSupported\":\"openid\"}"
                        |> Expect.equal
                            (Ok
                                { configId = Just 4
                                , outcome =
                                    Ok
                                        { body = "{}"
                                        , authorizationEndpoint = "http://localhost:9500/authorize"
                                        , tokenEndpoint = "http://localhost:9500/token"
                                        , scopesSupported = "openid"
                                        }
                                }
                            )
            ]
        ]
