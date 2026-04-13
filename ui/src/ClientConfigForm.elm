module ClientConfigForm exposing (Action(..), Model, Msg, init, initFromEdit, initFromImport, update, view)

import Html exposing (..)
import Html.Attributes exposing (class, placeholder, selected, type_, value)
import Html.Events exposing (onClick, onInput)


-- MODEL


type alias ExtraParam =
    { key : String
    , value : String
    }


type alias Model =
    { editingId : Maybe String
    , name : String
    , issuerUrl : String
    , authorizationUrl : String
    , tokenUrl : String
    , clientId : String
    , clientSecret : String
    , scopes : String
    , grantType : String
    , extraParams : List ExtraParam
    , disabledParams : String
    }


init : Model
init =
    { editingId = Nothing
    , name = ""
    , issuerUrl = ""
    , authorizationUrl = ""
    , tokenUrl = ""
    , clientId = ""
    , clientSecret = ""
    , scopes = ""
    , grantType = "authorization_code"
    , extraParams = []
    , disabledParams = "{}"
    }


initFromImport : { name : String, issuerUrl : String, authorizationUrl : String, tokenUrl : String, clientId : String, clientSecret : String } -> Model
initFromImport data =
    { editingId = Nothing
    , name = data.name
    , issuerUrl = data.issuerUrl
    , authorizationUrl = data.authorizationUrl
    , tokenUrl = data.tokenUrl
    , clientId = data.clientId
    , clientSecret = data.clientSecret
    , scopes = ""
    , grantType = "authorization_code"
    , extraParams = []
    , disabledParams = "{}"
    }


initFromEdit : { id : String, name : String, issuerUrl : String, authorizationUrl : String, tokenUrl : String, clientId : String, clientSecret : String, scopes : String, grantType : String, extraParams : String, disabledParams : String } -> Model
initFromEdit data =
    { editingId = Just data.id
    , name = data.name
    , issuerUrl = data.issuerUrl
    , authorizationUrl = data.authorizationUrl
    , tokenUrl = data.tokenUrl
    , clientId = data.clientId
    , clientSecret = data.clientSecret
    , scopes = data.scopes
    , grantType = data.grantType
    , extraParams = parseExtraParams data.extraParams
    , disabledParams = data.disabledParams
    }


parseExtraParams : String -> List ExtraParam
parseExtraParams str =
    if String.isEmpty str then
        []

    else
        let
            trimmed =
                str
                    |> String.trim
                    |> (\s ->
                            if String.startsWith "{" s && String.endsWith "}" s then
                                String.slice 1 (String.length s - 1) s

                            else
                                s
                       )

            pairs =
                String.split "," trimmed
        in
        List.filterMap
            (\pair ->
                case String.split ":" pair of
                    [ k, v ] ->
                        Just
                            { key = String.trim k |> stripQuotes
                            , value = String.trim v |> stripQuotes
                            }

                    _ ->
                        Nothing
            )
            pairs


stripQuotes : String -> String
stripQuotes s =
    if String.startsWith "\"" s && String.endsWith "\"" s then
        String.slice 1 (String.length s - 1) s

    else
        s



-- UPDATE


type Msg
    = SetName String
    | SetIssuerUrl String
    | SetAuthorizationUrl String
    | SetTokenUrl String
    | SetClientId String
    | SetClientSecret String
    | SetScopes String
    | SetGrantType String
    | AddExtraParam
    | RemoveExtraParam Int
    | SetExtraParamKey Int String
    | SetExtraParamValue Int String
    | Submit
    | Cancel


type Action
    = SubmitForm
    | CancelForm


update : Msg -> Model -> ( Model, Maybe Action )
update msg model =
    case msg of
        SetName val ->
            ( { model | name = val }, Nothing )

        SetIssuerUrl val ->
            ( { model | issuerUrl = val }, Nothing )

        SetAuthorizationUrl val ->
            ( { model | authorizationUrl = val }, Nothing )

        SetTokenUrl val ->
            ( { model | tokenUrl = val }, Nothing )

        SetClientId val ->
            ( { model | clientId = val }, Nothing )

        SetClientSecret val ->
            ( { model | clientSecret = val }, Nothing )

        SetScopes val ->
            ( { model | scopes = val }, Nothing )

        SetGrantType val ->
            ( { model | grantType = val }, Nothing )

        AddExtraParam ->
            ( { model | extraParams = model.extraParams ++ [ { key = "", value = "" } ] }, Nothing )

        RemoveExtraParam idx ->
            ( { model | extraParams = removeAt idx model.extraParams }, Nothing )

        SetExtraParamKey idx val ->
            ( { model | extraParams = updateAt idx (\p -> { p | key = val }) model.extraParams }, Nothing )

        SetExtraParamValue idx val ->
            ( { model | extraParams = updateAt idx (\p -> { p | value = val }) model.extraParams }, Nothing )

        Submit ->
            ( model, Just SubmitForm )

        Cancel ->
            ( model, Just CancelForm )


removeAt : Int -> List a -> List a
removeAt idx list =
    List.indexedMap Tuple.pair list
        |> List.filterMap
            (\( i, item ) ->
                if i == idx then
                    Nothing

                else
                    Just item
            )


updateAt : Int -> (a -> a) -> List a -> List a
updateAt idx fn list =
    List.indexedMap
        (\i item ->
            if i == idx then
                fn item

            else
                item
        )
        list


extraParamsToJson : List ExtraParam -> String
extraParamsToJson params =
    let
        nonEmpty =
            List.filter (\p -> not (String.isEmpty p.key)) params

        pairs =
            List.map (\p -> "\"" ++ p.key ++ "\":\"" ++ p.value ++ "\"") nonEmpty
    in
    if List.isEmpty pairs then
        ""

    else
        "{" ++ String.join "," pairs ++ "}"



-- VIEW


view : String -> Model -> Html Msg
view callbackUrl model =
    let
        ( title, submitLabel ) =
            case model.editingId of
                Just _ ->
                    ( "Edit Client Configuration", "Save Config" )

                Nothing ->
                    ( "New Client Configuration", "Create Config" )
    in
    div [ class "form-overlay" ]
        [ div [ class "form-panel" ]
            [ div [ class "form-title" ] [ text title ]
            , viewInput "Name" model.name SetName "My Client Config"
            , viewInput "Issuer URL" model.issuerUrl SetIssuerUrl "https://auth.example.com"
            , viewInput "Authorization URL" model.authorizationUrl SetAuthorizationUrl "https://auth.example.com/authorize"
            , viewInput "Token URL" model.tokenUrl SetTokenUrl "https://auth.example.com/token"
            , viewInput "Client ID" model.clientId SetClientId "client-id"
            , viewInput "Client Secret" model.clientSecret SetClientSecret "client-secret"
            , viewInput "Scopes" model.scopes SetScopes "openid profile email"
            , viewGrantType model.grantType
            , if model.grantType == "authorization_code" && not (String.isEmpty callbackUrl) then
                viewReadonlyField "Callback URL" callbackUrl

              else
                text ""
            , viewExtraParams model.extraParams
            , div [ class "form-actions" ]
                [ button [ class "btn-cancel", onClick Cancel ] [ text "Cancel" ]
                , button [ class "btn-submit", onClick Submit ] [ text submitLabel ]
                ]
            ]
        ]


viewInput : String -> String -> (String -> Msg) -> String -> Html Msg
viewInput label val toMsg hint =
    div [ class "form-field" ]
        [ span [ class "form-label" ] [ text label ]
        , input [ class "form-input", type_ "text", value val, placeholder hint, onInput toMsg ] []
        ]


viewReadonlyField : String -> String -> Html Msg
viewReadonlyField label val =
    div [ class "form-field" ]
        [ span [ class "form-label" ] [ text label ]
        , input [ class "form-input form-input-readonly", type_ "text", value val, Html.Attributes.readonly True ] []
        ]


viewGrantType : String -> Html Msg
viewGrantType current =
    div [ class "form-field" ]
        [ span [ class "form-label" ] [ text "Grant Type" ]
        , Html.select [ class "form-input form-select", onInput SetGrantType ]
            [ option [ value "authorization_code", selected (current == "authorization_code") ] [ text "Authorization Code" ]
            , option [ value "client_credentials", selected (current == "client_credentials") ] [ text "Client Credentials" ]
            , option [ value "implicit", selected (current == "implicit") ] [ text "Implicit" ]
            , option [ value "password", selected (current == "password") ] [ text "Password" ]
            ]
        ]


viewExtraParams : List ExtraParam -> Html Msg
viewExtraParams params =
    div [ class "form-field" ]
        [ div [ class "extra-params-header" ]
            [ span [ class "form-label" ] [ text "Extra Query Parameters" ]
            , button [ class "btn-add-client", onClick AddExtraParam ] [ text "+ Add" ]
            ]
        , div [ class "extra-params-list" ]
            (List.indexedMap viewExtraParam params)
        ]


viewExtraParam : Int -> ExtraParam -> Html Msg
viewExtraParam idx param =
    div [ class "extra-param-row" ]
        [ input [ class "form-input extra-param-key", type_ "text", value param.key, placeholder "key", onInput (SetExtraParamKey idx) ] []
        , input [ class "form-input extra-param-value", type_ "text", value param.value, placeholder "value", onInput (SetExtraParamValue idx) ] []
        , button [ class "btn-delete", onClick (RemoveExtraParam idx) ] [ text "\u{1F5D1}" ]
        ]
